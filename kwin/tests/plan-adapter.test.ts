import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";

import {
    DirectionalObservation,
    KeepAboveWriteOutcome,
    MaximizeClearOutcome,
    PLAN_CONTRACT_VERSION,
    PLAN_DEBOUNCE_MS,
    PLAN_INTERFACE,
    PLAN_METHOD,
    PLAN_OBJECT,
    PLAN_SERVICE,
    PlanAdapter,
    PlanAdapterEnv,
    PlanDirection,
    PlanObserved,
    planFingerprint,
} from "../src/plan-adapter";
import { planShortcutCatalog, startPlanAdapterEntry } from "../src/plan-adapter-entry";
import { DOMAIN_GAP, OUTER_DOMAIN_GAP } from "../src/domain-gap";
import type { PlanEntryOverrides } from "../src/plan-adapter-entry";

function kwinSrcDir(): string {
    const override = process.env["KWIN_SRC_DIR"];
    if (typeof override === "string" && override.length > 0) {
        try {
            if (existsSync(join(override, "entry.ts"))) {
                return override;
            }
        } catch (error) {
            void error;
        }
    }
    const candidates: string[] = [];
    try {
        const here: unknown = typeof __dirname === "string" ? __dirname : process.cwd();
        if (typeof here === "string") {
            candidates.push(resolve(here, "..", "..", "src"));
            candidates.push(resolve(here, "..", "src"));
            candidates.push(resolve(here, "src"));
        }
    } catch (error) {
        void error;
    }
    candidates.push(resolve(process.cwd(), "src"));
    candidates.push(resolve(process.cwd(), "kwin", "src"));
    for (const dir of candidates) {
        try {
            if (existsSync(join(dir, "entry.ts"))) {
                return dir;
            }
        } catch (error) {
            void error;
        }
    }
    return resolve(process.cwd(), "src");
}

declare const __dirname: string | undefined;

function makeRefs(): { a: object; b: object; c: object } {
    return { a: {}, b: {}, c: {} };
}

function makeObserved(
    refs: { a: object; b: object; c: object },
    opts: {
        focused?: object;
        rects?: Record<string, { x: number; y: number; w: number; h: number }>;
        workspaces?: Record<string, string>;
        fullscreen?: Record<string, boolean>;
        maximized?: Record<string, boolean>;
        floating?: Record<string, boolean>;
        sticky?: Record<string, boolean>;
        resourceClasses?: Record<string, string>;
        fingerprint?: string;
        revalidate?: () => boolean;
        bounds?: { x: number; y: number; w: number; h: number };
        domainGap?: number;
        domainOuterGap?: number;
    } = {},
): PlanObserved {
    const focused = opts.focused ?? refs.a;
    const rect = (id: string): { x: number; y: number; w: number; h: number } =>
        opts.rects?.[id] ?? { x: 0, y: 0, w: 100, h: 100 };
    const workspace = (id: string): string => opts.workspaces?.[id] ?? "ws-1";
    const isFullscreen = (id: string): boolean => opts.fullscreen?.[id] === true;
    const isMaximized = (id: string): boolean => opts.maximized?.[id] === true;
    const isFloating = (id: string): boolean => opts.floating?.[id] === true;
    const isSticky = (id: string): boolean => opts.sticky?.[id] === true;
    const resourceClass = (id: string): string => opts.resourceClasses?.[id] ?? "unknown";
    const windows = Object.freeze([
        Object.freeze({ id: "win-a", ref: refs.a, rect: rect("win-a"), output: "out-1", workspace: workspace("win-a"), fullscreen: isFullscreen("win-a"), maximized: isMaximized("win-a"), floating: isFloating("win-a"), sticky: isSticky("win-a"), resourceClass: resourceClass("win-a") }),
        Object.freeze({ id: "win-b", ref: refs.b, rect: rect("win-b"), output: "out-1", workspace: workspace("win-b"), fullscreen: isFullscreen("win-b"), maximized: isMaximized("win-b"), floating: isFloating("win-b"), sticky: isSticky("win-b"), resourceClass: resourceClass("win-b") }),
    ]);
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-1",
        domainBounds: opts.bounds ?? { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: opts.domainGap ?? DOMAIN_GAP,
        domainOuterGap: opts.domainOuterGap ?? OUTER_DOMAIN_GAP,
        focusedId: focused === refs.a ? "win-a" : "win-b",
        windows,
        activeRef: focused,
        fingerprint: opts.fingerprint ?? "fp-1",
        revalidate: opts.revalidate ?? (() => true),
    };
}

interface Mocks {
    readonly dbusCalls: Array<{ service: string; path: string; iface: string; method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly subscribes: Array<{ kind: string; handler: (target?: object) => void }>;
    readonly geometries: Array<{ target: object; rect: { x: number; y: number; w: number; h: number } }>;
    readonly actives: object[];
    readonly desktopsWrites: Array<{ mover: object; desktops: ReadonlyArray<object> }>;
    readonly maximizeClears: object[];
    readonly maximizeToggles: Array<{ target: object; maximized: boolean }>;
    readonly fullscreenToggles: Array<{ target: object; fullscreen: boolean }>;
    readonly desktopToggles: Array<{ target: object; allDesktops: boolean }>;
    readonly keepAboveToggles: Array<{ target: object; keepAbove: boolean }>;
    readonly keepBelowToggles: Array<{ target: object; keepBelow: boolean }>;
    readonly floatingCalls: Array<{ id: string; floating: boolean }>;
    observeImpl: () => PlanObserved | null;
    activeImpl: () => object | null;
    bornMode: unknown;
    geometryImpl: (target: object, rect: { x: number; y: number; w: number; h: number }) => boolean;
    maximizeClearImpl: (target: object) => MaximizeClearOutcome;
    maximizeToggleImpl: (target: object, maximized: boolean) => MaximizeClearOutcome;
    fullscreenToggleImpl: (target: object, fullscreen: boolean) => MaximizeClearOutcome;
    desktopToggleImpl: (target: object, allDesktops: boolean) => MaximizeClearOutcome;
    keepAboveReadImpl: (target: object) => boolean | null;
    keepAboveToggleImpl: (target: object, keepAbove: boolean) => KeepAboveWriteOutcome;
    keepBelowReadImpl: (target: object) => boolean | null;
    keepBelowToggleImpl: (target: object, keepBelow: boolean) => KeepAboveWriteOutcome;
    env: PlanAdapterEnv;
}

function mockEnv(refs: { a: object; b: object; c: object }): Mocks {
    const state: Mocks = {
        dbusCalls: [],
        callbacks: [],
        timers: [],
        logs: [],
        subscribes: [],
        geometries: [],
        actives: [],
        desktopsWrites: [],
        maximizeClears: [],
        maximizeToggles: [],
        fullscreenToggles: [],
        desktopToggles: [],
        keepAboveToggles: [],
        keepBelowToggles: [],
        floatingCalls: [],
        observeImpl: () => makeObserved(refs, { focused: refs.a }),
        activeImpl: () => refs.a,
        // R-MAX-06: the production default is tile-on-launch; tests
        // asserting the Q3 reserved-slot overlay path pin
        // keep-maximized-overlay explicitly.
        bornMode: undefined as unknown,
        geometryImpl: (_target: object, _rect: { x: number; y: number; w: number; h: number }): boolean => true,
        maximizeClearImpl: (_target: object): MaximizeClearOutcome => "invoked",
        maximizeToggleImpl: (_target: object, _maximized: boolean): MaximizeClearOutcome => "invoked",
        fullscreenToggleImpl: (_target: object, _fullscreen: boolean): MaximizeClearOutcome => "invoked",
        desktopToggleImpl: (_target: object, _allDesktops: boolean): MaximizeClearOutcome => "invoked",
        keepAboveReadImpl: (_target: object): boolean | null => false,
        keepAboveToggleImpl: (_target: object, _keepAbove: boolean): KeepAboveWriteOutcome => "invoked",
        keepBelowReadImpl: (_target: object): boolean | null => false,
        keepBelowToggleImpl: (_target: object, _keepBelow: boolean): KeepAboveWriteOutcome => "invoked",
        env: null as unknown as PlanAdapterEnv,
    };
    const env: PlanAdapterEnv = {
        callDbus: (service, path, iface, method, payload, callback): void => {
            // Owner-pinned transport: answer bus-daemon activation
            // synchronously without recording so legacy DescribePlan
            // assertions keep their indices; new recovery tests cover the
            // activation steps explicitly.
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
            state.dbusCalls.push({ service, path, iface, method, payload });
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
        readBornMaximized: (): unknown => state.bornMode,
        clearMaximize: (target): MaximizeClearOutcome => {
            state.maximizeClears.push(target);
            return state.maximizeClearImpl(target);
        },
        setMaximize: (target, maximized): MaximizeClearOutcome => {
            state.maximizeToggles.push({ target, maximized });
            return state.maximizeToggleImpl(target, maximized);
        },
        setFullscreen: (target, fullscreen): MaximizeClearOutcome => {
            state.fullscreenToggles.push({ target, fullscreen });
            return state.fullscreenToggleImpl(target, fullscreen);
        },
        setAllDesktops: (target, allDesktops): MaximizeClearOutcome => {
            state.desktopToggles.push({ target, allDesktops });
            return state.desktopToggleImpl(target, allDesktops);
        },
        readKeepAbove: (target): boolean | null => state.keepAboveReadImpl(target),
        readKeepBelow: (target): boolean | null => state.keepBelowReadImpl(target),
        setKeepAbove: (target, keepAbove): KeepAboveWriteOutcome => {
            state.keepAboveToggles.push({ target, keepAbove });
            return state.keepAboveToggleImpl(target, keepAbove);
        },
        setKeepBelow: (target, keepBelow): KeepAboveWriteOutcome => {
            state.keepBelowToggles.push({ target, keepBelow });
            return state.keepBelowToggleImpl(target, keepBelow);
        },
        setGeometry: (target, rect): boolean => {
            state.geometries.push({ target, rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h } });
            return state.geometryImpl(target, rect);
        },
        setFloating: (id, floating): void => {
            state.floatingCalls.push({ id, floating });
        },
        setActive: (target): boolean => {
            state.actives.push(target);
            return true;
        },
        setDesktops: (mover, desktops): boolean => {
            state.desktopsWrites.push({ mover, desktops });
            return true;
        },
        active: (): object | null => state.activeImpl(),
        subscribe: (kind, handler): (() => void) => {
            state.subscribes.push({ kind, handler });
            return (): void => {};
        },
    };
    state.env = env;
    return state;
}

function enableAdapter(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function plannerPayload(mocks: Mocks, index: number): Record<string, unknown> {
    return JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
}

function plannedReply(
    correlation: string,
    geom: ReadonlyArray<{ window: string; rect: { x: number; y: number; w: number; h: number } }>,
    focusLeaf: string | null,
): string {
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "planned",
        base_revision: 2,
        detail: { kind: "focus" },
        desired_geometry: geom.map((entry) => ({
            window: entry.window,
            leaf: `${entry.window}-leaf`,
            output: "out-1",
            workspace: "ws-1",
            rect: entry.rect,
        })),
        desired_focus:
            focusLeaf === null
                ? undefined
                : { domain_output: "out-1", domain_workspace: "ws-1", leaf: focusLeaf },
    });
}

function rejectedReply(correlation: string, kind: string): string {
    return JSON.stringify({ v: 1, correlation_id: correlation, outcome: "rejected", kind, message: "no" });
}

function fire(mocks: Mocks, kind: string, target?: object): void {
    for (const sub of mocks.subscribes) {
        if (sub.kind === kind) {
            sub.handler(target);
        }
    }
}

function runTimers(mocks: Mocks): void {
    const pending = [...mocks.timers];
    mocks.timers.length = 0;
    for (const timer of pending) {
        if (!timer.cancelled) {
            timer.callback();
        }
    }
}

function runDebounce(mocks: Mocks): void {
    const pending = [...mocks.timers];
    mocks.timers.length = 0;
    for (const timer of pending) {
        if (timer.cancelled) {
            continue;
        }
        if (timer.delayMs === PLAN_DEBOUNCE_MS) {
            timer.callback();
        } else {
            mocks.timers.push(timer);
        }
    }
}

describe("plan adapter route identity and request shape", () => {
    it("invokes only DescribePlan with a single JSON string payload", () => {
        assert.equal(PLAN_SERVICE, "com.omnitiler.Planner");
        assert.equal(PLAN_OBJECT, "/com/omnitiler/Planner");
        assert.equal(PLAN_INTERFACE, "com.omnitiler.Planner1");
        assert.equal(PLAN_METHOD, "DescribePlan");
        assert.equal(PLAN_CONTRACT_VERSION, 1);
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        const call = mocks.dbusCalls[0] as { service: string; path: string; iface: string; method: string };
        // Owner-pinned transport: the planner method targets the pinned
        // unique owner, never the well-known name.
        assert.match(call.service, /^:[0-9]+\.[0-9]+$/);
        assert.equal(call.path, PLAN_OBJECT);
        assert.equal(call.iface, PLAN_INTERFACE);
        assert.equal(call.method, PLAN_METHOD);
        const payload = plannerPayload(mocks, 0);
        assert.equal(payload["v"], 1);
        assert.equal(payload["owner"], "owner-1");
        assert.equal(payload["generation"], "gen-1");
        assert.equal(payload["revision"], 0);
        assert.equal(typeof payload["fingerprint"], "number");
        assert.deepEqual(payload["domain"], {
            output: "out-1",
            workspace: "ws-1",
            bounds: { x: 0, y: 0, w: 1200, h: 800 },
            gap: DOMAIN_GAP,
            outer_gap: OUTER_DOMAIN_GAP,
        });
        assert.equal(payload["focused_window"], "win-a");
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 2);
        assert.deepEqual(windows[0], {
            window: "win-a",
            output: "out-1",
            workspace: "ws-1",
            rect: { x: 0, y: 0, w: 100, h: 100 },
        });
        assert.deepEqual(payload["command"], { op: "focus", window: "win-a", direction: "left" });
    });

    it("marks floating, sticky, fullscreen, and maximized members fit_excluded", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const classes = ["floating", "sticky", "fullscreen"] as const;
        classes.forEach((cls, index) => {
            mocks.observeImpl = () => makeObserved(refs, { [cls]: { "win-a": true } });
            adapter.requestFocus("left");
            const payload = plannerPayload(mocks, index);
            const windows = payload["windows"] as Array<Record<string, unknown>>;
            assert.equal(windows.length, 2);
            assert.equal(
                (windows.find((entry) => entry["window"] === "win-a") as Record<string, unknown>)["fit_excluded"],
                true,
            );
            assert.equal(
                "fit_excluded" in (windows.find((entry) => entry["window"] === "win-b") as Record<string, unknown>),
                false,
            );
            mocks.callbacks[index]?.(rejectedReply(payload["correlation_id"] as string, "snapshot-invalid"));
        });
        // G-06 (REQ-MAX-08): a maximized FOCUSED window fences focus with no
        // dispatch; the maximized flag stays local snapshot semantics for
        // sibling overlays alongside the portable fit exclusion.
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": true } });
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, classes.length, "fenced maximized focus dispatches nothing");
        assert.ok(mocks.logs.includes("omnitiler:plan:focus-refused-maximize"));
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-b": true } });
        adapter.requestFocus("left");
        const overlay = plannerPayload(mocks, classes.length);
        const overlayWindows = overlay["windows"] as Array<Record<string, unknown>>;
        const sibling = overlayWindows.find((entry) => entry["window"] === "win-b") as Record<string, unknown>;
        assert.equal("maximized" in sibling, false, "sibling overlay keeps maximized local to the snapshot");
        assert.equal(sibling["fit_excluded"], true);
        mocks.callbacks[classes.length]?.(rejectedReply(overlay["correlation_id"] as string, "snapshot-invalid"));
        // Clean tiled members omit the marker entirely.
        mocks.observeImpl = () => makeObserved(refs, {});
        adapter.requestFocus("left");
        const clean = plannerPayload(mocks, classes.length + 1)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(clean[0], {
            window: "win-a",
            output: "out-1",
            workspace: "ws-1",
            rect: { x: 0, y: 0, w: 100, h: 100 },
        });
    });

    it("sends the deterministic numeric fingerprint binding", () => {
        const expected = planFingerprint("out-1", "ws-1", "win-a", ["win-a", "win-b"]);
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        assert.equal(plannerPayload(mocks, 0)["fingerprint"], expected);
        assert.deepEqual(plannerPayload(mocks, 0)["command"], { op: "move", window: "win-a", direction: "right" });
    });

    it("parameterizes resize with direction, mode, and repeat press index", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestResize("left", "outwards");
        assert.deepEqual(plannerPayload(mocks, 0)["command"], {
            op: "resize",
            window: "win-a",
            direction: "left",
            mode: "outwards",
            press_index: 0,
        });
        mocks.callbacks[0]?.(rejectedReply(plannerPayload(mocks, 0)["correlation_id"] as string, "snapshot-invalid"));
        adapter.requestResize("left", "outwards");
        assert.equal((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["press_index"], 1);
        mocks.callbacks[1]?.(rejectedReply(plannerPayload(mocks, 1)["correlation_id"] as string, "snapshot-invalid"));
        adapter.requestResize("left", "inwards");
        assert.equal((plannerPayload(mocks, 2)["command"] as Record<string, unknown>)["press_index"], 0);
    });
});

describe("plan adapter geometry application", () => {
    it("applies complete reply geometries in grow-before-shrink order with move", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 500 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 100, h: 100 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.equal(mocks.geometries.length, 2);
        assert.equal(mocks.geometries[0]?.target, refs.a);
        assert.equal(mocks.geometries[1]?.target, refs.b);
        assert.deepEqual(mocks.actives, [refs.b]);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
    });

    it("focus never rewrites geometry and only moves the active window", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("right");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 500 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 100, h: 100 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.equal(mocks.geometries.length, 0);
        assert.deepEqual(mocks.actives, [refs.b]);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
    });

    it("skips unchanged windows and skips redundant focus writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        mocks.activeImpl = () => refs.b;
        const adapter = enableAdapter(mocks);
        adapter.requestMove("left");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 100, h: 100 } },
                    { window: "win-b", rect: { x: 100, y: 0, w: 400, h: 500 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.geometries[0]?.target, refs.b);
        assert.equal(mocks.actives.length, 0);
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:write window=win-a resource_class=unknown disposition=skip-already-equal rect=0,0,100,100",
            ),
            "unchanged member carries skip-already-equal disposition",
        );
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:write window=win-b resource_class=unknown disposition=written rect=100,0,400,500",
            ),
            "changed member carries written disposition with the target rect",
        );
    });

    it("traces native constraints across a write and later contained height mismatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let observedB = { x: 100, y: 0, w: 500, h: 500 };
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": observedB,
                },
            });
        mocks.activeImpl = () => refs.b;
        const env: PlanAdapterEnv = {
            ...mocks.env,
            setGeometry: (target, rect): boolean => {
                const written = mocks.env.setGeometry(target, rect);
                if (target === refs.b && written) {
                    observedB = { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
                }
                return written;
            },
            readGeometry: (target) => (target === refs.b ? observedB : { x: 0, y: 0, w: 100, h: 100 }),
            readWindowConstraints: (target) => {
                if (target === refs.a) {
                    throw new Error("constraint getter failed");
                }
                return {
                    resizeable: true,
                    minSize: { w: 80, h: 60 },
                    maxSize: { w: 1600, h: 900 },
                };
            },
        };
        const adapter = new PlanAdapter(env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        adapter.requestMove("left");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 100, h: 100 } },
                    { window: "win-b", rect: { x: 100, y: 0, w: 400, h: 500 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line ===
                    `omnitiler:plan:constraint-trace corr=${correlation} phase=plan window=win-b output=out-1 resource_class=unknown resizeable=true min=80,60 max=1600,900 workarea=0,0,1200,800 requested=100,0,400,500 observed=100,0,500,500`,
            ),
        );
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line ===
                    `omnitiler:plan:constraint-trace corr=${correlation} phase=write window=win-b output=out-1 resource_class=unknown resizeable=true min=80,60 max=1600,900 workarea=0,0,1200,800 requested=100,0,400,500 observed=100,0,400,500`,
            ),
        );
        // Auto no longer emits pre-plan/post-signal: the bounded trace is
        // exactly plan/write for the applied member, with redacted identity.
        observedB = { x: 100, y: 0, w: 400, h: 450 };
        mocks.subscribes.find((entry) => entry.kind === "geometry")?.handler(refs.b);
        mocks.timers[mocks.timers.length - 1]?.callback();
        assert.ok(
            mocks.logs.every((line) => !line.includes("phase=post-signal")),
            "auto removed post-signal trace",
        );
    });

    it("rejects partial or unknown reply windows without native writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(correlation, [{ window: "win-a", rect: { x: 0, y: 0, w: 200, h: 200 } }], null),
        );
        assert.equal(mocks.geometries.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=precondition-mismatch")));
        assert.equal(adapter.isEnabled, true);
    });

    it("records a write-failed disposition with the attempted rect on setGeometry failure", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.geometryImpl = () => false;
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                null,
            ),
        );
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:write window=win-a resource_class=unknown disposition=write-failed rect=0,0,600,800",
            ),
            "failed member carries write-failed disposition",
        );
        assert.ok(mocks.logs.some((line) => line.includes("outcome=write-failed")));
    });
});

describe("plan adapter recovery and fencing", () => {
    it("keeps rejections recoverable with a bounded rejection-kind line", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        const first = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(rejectedReply(first, "snapshot-invalid"));
        assert.equal(adapter.isEnabled, true);
        assert.equal(adapter.isInFlight, false);
        assert.ok(
            mocks.logs.some((line) => line.includes("outcome=rejected") && line.includes(`cmd=${first}`)),
        );
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:rejected kind=snapshot-invalid"));
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2);
    });

    it("keeps a rejected reconcile out of the committed baseline so a reopened window is converged", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let ids = ["win-a", "win-b"];
        const byId: Record<string, object> = {
            "win-a": refs.a,
            "win-b": refs.b,
            "win-c": refs.c,
            "win-c-reopened": refs.c,
        };
        mocks.observeImpl = (): PlanObserved => {
            const windows = ids.map((id) =>
                Object.freeze({
                    id,
                    ref: byId[id] as object,
                    rect:
                        id === "win-a"
                            ? { x: 0, y: 0, w: 600, h: 800 }
                            : id === "win-b"
                              ? { x: 600, y: 0, w: 600, h: 800 }
                              : id === "win-c"
                                ? { x: 0, y: 0, w: 1200, h: 800 }
                                : { x: 800, y: 0, w: 400, h: 800 },
                    output: "out-1",
                    workspace: "ws-1",
                    fullscreen: false,
                    maximized: false,
                }),
            );
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-a",
                windows: Object.freeze(windows),
                activeRef: refs.a,
                fingerprint: `fp-${ids.join(",")}`,
                revalidate: () => true,
            };
        };
        const adapter = enableAdapter(mocks);

        fire(mocks, "added");
        runTimers(mocks);
        let correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );

        ids = ["win-a", "win-b", "win-c"];
        fire(mocks, "added");
        runTimers(mocks);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        assert.ok(
            ((plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"])).includes("win-c"),
            "reconcile carries the complete observation including the newcomer",
        );
        correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "rejected",
                kind: "snapshot-invalid",
                detail: "window-out-of-bounds",
                message: "no",
            }),
        );
        assert.ok(
            mocks.logs.some(
                (line) => line === "omnitiler:plan:rejected kind=snapshot-invalid detail=window-out-of-bounds",
            ),
        );

        ids = ["win-a", "win-b"];
        fire(mocks, "removed");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "the rejected window was never treated as committed");

        ids = ["win-a", "win-b", "win-c-reopened"];
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 3);
        assert.deepEqual(plannerPayload(mocks, 2)["command"], { op: "reconcile" });
        assert.ok(
            ((plannerPayload(mocks, 2)["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"])).includes("win-c-reopened"),
            "reconcile carries the complete observation including the reopened window",
        );
        correlation = plannerPayload(mocks, 2)["correlation_id"] as string;
        mocks.callbacks[2]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c-reopened", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-reopened-leaf",
            ),
        );
        assert.ok(
            mocks.logs.some((line) => line.includes("cmd=gen-1-p2") && line.includes("outcome=planned-applied")),
            mocks.logs.join("\n"),
        );
        assert.equal(adapter.isEnabled, true);
    });

    it("fences stale replies against a newer debounced observation", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let version = 0;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                fingerprint: version === 0 ? "fp-1" : "fp-2",
                rects:
                    version === 0
                        ? { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } }
                        : { "win-a": { x: 5, y: 5, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        version = 1;
        fire(mocks, "geometry");
        runDebounce(mocks);
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 200, h: 200 } },
                    { window: "win-b", rect: { x: 200, y: 0, w: 100, h: 100 } },
                ],
                null,
            ),
        );
        assert.equal(mocks.geometries.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=stale-dropped")));
        assert.equal(adapter.isEnabled, true);
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 2);
    });

    it("drops a planned reply when a member flips floating mid-flight without native writes", () => {
        // Race fixture: dispatch observes both members tiled, then native
        // float takes effect before the reply. The reply geometry still
        // covers the dispatch set, so only flag-exact revalidation can fail
        // it closed; a stale apply would write the floated window.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floatingA = false;
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, floating: { "win-a": floatingA } });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        floatingA = true;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 200, h: 200 } },
                    { window: "win-b", rect: { x: 200, y: 0, w: 100, h: 100 } },
                ],
                null,
            ),
        );
        assert.equal(mocks.geometries.length, 0, "no stale geometry reaches a newly floating window");
        assert.equal(mocks.actives.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=stale-scope")));
        assert.equal(adapter.isEnabled, true);
    });

    it("fresh no-baseline all-floating foreground observation sends no admit", () => {
        // Complete-observation convergence: a fresh foreground observation
        // with no applied evidence whose members are all floating/sticky
        // exceptions converges through one reconcile carrying the complete
        // float-only observation; the Engine records exceptions and projects
        // no tiles, so no native focus/write happens and never an admit.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, floating: { "win-a": true, "win-b": true }, sticky: { "win-a": true } });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "all-floating fresh observation converges via reconcile");
        const payload = plannerPayload(mocks, 0);
        assert.deepEqual((payload["command"] as Record<string, unknown>)["op"], "reconcile");
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 2, "reconcile carries the complete float-only observation");
        for (const entry of windows) {
            assert.equal(entry["floating"], true, "float rides as floating evidence, not a tile");
            assert.equal(entry["fit_excluded"], true, "exception member stays fit-excluded");
        }
        assert.ok(
            mocks.dbusCalls.every((call) => (JSON.parse(call.payload) as Record<string, unknown>)["command"] !== undefined && ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] !== "admit"),
            "all-floating converge never admits",
        );
        assert.equal(mocks.geometries.length, 0, "float-only converge writes no geometry");
        assert.equal(mocks.actives.length, 0, "float-only converge writes no native focus");
        assert.equal(adapter.isEnabled, true);
    });

    it("refuses busy shortcut commands with one bounded refusal line per kind", () => {        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        const logsBefore = mocks.logs.length;
        adapter.requestMove("right");
        adapter.requestResize("left", "outwards");
        assert.equal(mocks.dbusCalls.length, 1);
        assert.equal(mocks.logs.length, logsBefore + 2);
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:busy-refused kind=move"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:busy-refused kind=resize"));
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:busy-refused kind=focus"));
    });

    it("drives reconcile from debounced membership diffs", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let ids: string[] = ["win-a", "win-b"];
        const byId: Record<string, object> = { "win-a": refs.a, "win-b": refs.b, "win-c": refs.c };
        mocks.observeImpl = (): PlanObserved | null => {
            const wins = ids.map((id) =>
                Object.freeze({ id, ref: byId[id] as object, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
            );
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-a",
                windows: Object.freeze(wins),
                activeRef: refs.a,
                fingerprint: `fp-${ids.join(",")}`,
                revalidate: () => true,
            };
        };
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.isInFlight, false);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        assert.deepEqual((plannerPayload(mocks, 0)["command"] as Record<string, unknown>)["op"], "reconcile");
        const initialCorrelation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                initialCorrelation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        ids = ["win-a", "win-b", "win-c"];
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        assert.ok(
            ((plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"])).includes("win-c"),
            "newcomer reconcile carries the complete observation",
        );
        const admitCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.equal(mocks.geometries.length, 5);
        ids = ["win-a", "win-b"];
        fire(mocks, "removed");
        runTimers(mocks);
        assert.ok(mocks.dbusCalls.length >= 3);
        const removeIndex = mocks.dbusCalls.length - 1;
        const removeCmd = plannerPayload(mocks, removeIndex)["command"] as Record<string, unknown>;
        assert.deepEqual(removeCmd, { op: "reconcile" });
        const removePayload = plannerPayload(mocks, removeIndex);
        const removeWindows = removePayload["windows"] as Array<Record<string, unknown>>;
        assert.ok(
            !removeWindows.some((entry) => entry["window"] === "win-c"),
            "remove carries the current post-removal snapshot omitting the departed member",
        );
        assert.ok(
            removeWindows.some((entry) => entry["window"] === "win-a") &&
                removeWindows.some((entry) => entry["window"] === "win-b"),
            "remove geometry covers the converged survivors",
        );
        assert.equal(removePayload["focused_window"], "win-a", "post-removal focus stays on an observed survivor");
    });

    it("retains per-workspace baselines across a rejected reconcile without duplicate probes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let workspace = "ws-a";
        let ids = ["win-a"];
        const byId: Record<string, object> = { "win-a": refs.a, "win-b": refs.b, "win-c": refs.c };
        mocks.observeImpl = (): PlanObserved | null => {
            const windows = ids.map((id) =>
                Object.freeze({
                    id,
                    ref: byId[id] as object,
                    rect: { x: 0, y: 0, w: 1200, h: 800 },
                    output: "out-1",
                    workspace,
                    fullscreen: false,
                    maximized: false,
                }),
            );
            return {
                domainOutput: "out-1",
                domainWorkspace: workspace,
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: ids[0] as string,
                windows: Object.freeze(windows),
                activeRef: byId[ids[0] as string] as object,
                fingerprint: `${workspace}-${ids.join(",")}`,
                revalidate: () => true,
            };
        };
        const adapter = enableAdapter(mocks);

        fire(mocks, "added");
        runTimers(mocks);
        let correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: { x: 0, y: 0, w: 1200, h: 800 } }], null));

        workspace = "ws-b";
        ids = ["win-c"];
        fire(mocks, "scope");
        runTimers(mocks);
        correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(plannedReply(correlation, [{ window: "win-c", rect: { x: 0, y: 0, w: 1200, h: 800 } }], null));

        workspace = "ws-a";
        ids = ["win-a"];
        fire(mocks, "scope");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "returning to an applied workspace does not re-admit known membership");

        workspace = "ws-b";
        ids = ["win-b", "win-c"];
        fire(mocks, "scope");
        runTimers(mocks);
        assert.deepEqual(plannerPayload(mocks, 2)["command"], { op: "reconcile" });
        mocks.callbacks[2]?.(rejectedReply(plannerPayload(mocks, 2)["correlation_id"] as string, "snapshot-invalid"));

        workspace = "ws-a";
        ids = ["win-a"];
        fire(mocks, "scope");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 3, "a rejected reconcile in another workspace cannot discard this baseline");
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:rejected kind=duplicate-window"));
        assert.equal(adapter.isEnabled, true);
    });
});

describe("foreground automatic reconcile gap retry", () => {
    function gapRejected(correlation: string, message: string): string {
        return JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "rejected",
            kind: "domain-mismatch",
            message,
        });
    }
    function freshReconcile(mocks: Mocks): string {
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        assert.deepEqual((plannerPayload(mocks, 0)["command"] as Record<string, unknown>)["op"], "reconcile");
        return plannerPayload(mocks, 0)["correlation_id"] as string;
    }
    it("retries one fresh same-domain update-gaps on an exact inner-gap mismatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const corr = freshReconcile(mocks);
        mocks.callbacks[0]?.(gapRejected(corr, "domain gap does not match retained state"));
        assert.equal(mocks.dbusCalls.length, 2, "exactly one update-gaps retry");
        const retry = plannerPayload(mocks, 1);
        assert.deepEqual(retry["command"], { op: "update-gaps" });
        assert.equal((retry["domain"] as Record<string, unknown>)["workspace"], "ws-1");
        assert.equal((retry["windows"] as Array<unknown>).length, 2, "retry carries the fresh complete observation");
        assert.ok(
            mocks.logs.some((line) => line.includes("gap-reprojection selected=retry")),
            "bounded retry diagnostic",
        );
        // A rejected retry never retries again.
        const retryCorr = retry["correlation_id"] as string;
        mocks.callbacks[1]?.(gapRejected(retryCorr, "domain outer gap does not match retained state"));
        assert.equal(mocks.dbusCalls.length, 2, "no retry from update-gaps");
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.isEnabled, true);
    });
    it("ignores a domain-mismatch refusal without the exact gap message", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const corr = freshReconcile(mocks);
        mocks.callbacks[0]?.(rejectedReply(corr, "domain-mismatch"));
        assert.equal(mocks.dbusCalls.length, 1, "kind-only mismatch never retries");
        assert.equal(adapter.isInFlight, false);
    });
    it("never retries from a cross-domain fresh observation", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const corr = freshReconcile(mocks);
        mocks.observeImpl = (): PlanObserved | null => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-2",
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: 0,
            domainOuterGap: 0,
            focusedId: "",
            windows: Object.freeze([]),
            activeRef: refs.a,
            fingerprint: "fp-other",
            revalidate: () => true,
        });
        mocks.callbacks[0]?.(gapRejected(corr, "domain gap does not match retained state"));
        assert.equal(mocks.dbusCalls.length, 1, "cross-domain evidence never retries");
        assert.equal(adapter.isInFlight, false);
    });
});
describe("plan adapter explicit-only floating", () => {
    it("never calls setFloating or emits toggle-float from automatic, admission, reconcile, or directional routes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const allocations = [
            { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
            { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
        ];

        // Automatic reconcile from a signal-driven membership diff.
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        assert.deepEqual((plannerPayload(mocks, 0)["command"] as Record<string, unknown>)["op"], "reconcile");
        mocks.callbacks[0]?.(
            plannedReply(plannerPayload(mocks, 0)["correlation_id"] as string, allocations, "win-a-leaf"),
        );

        // Directional focus, move, resize, and pointer-resize all complete
        // their flights through writeGeometries without any float transition.
        const directional = [
            () => adapter.requestFocus("right"),
            () => adapter.requestMove("right"),
            () => adapter.requestResize("left", "outwards"),
        ];
        for (const request of directional) {
            const before: number = mocks.dbusCalls.length;
            request();
            assert.equal(mocks.dbusCalls.length, before + 1);
            const index = mocks.dbusCalls.length - 1;
            assert.notEqual((plannerPayload(mocks, index)["command"] as Record<string, unknown>)["op"], "toggle-float");
            mocks.callbacks[index]?.(
                plannedReply(plannerPayload(mocks, index)["correlation_id"] as string, allocations, "win-a-leaf"),
            );
        }

        // Automatic reconcile after client geometry drift.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 200, h: 200 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        const reconcileIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, reconcileIndex)["command"] as Record<string, unknown>), { op: "reconcile" });
        mocks.callbacks[reconcileIndex]?.(
            plannedReply(plannerPayload(mocks, reconcileIndex)["correlation_id"] as string, allocations, "win-a-leaf"),
        );

        // Automatic reconcile again after a fresh window appears, completing
        // through writeGeometries rather than any float transition.
        mocks.observeImpl = () => {
            const wins = [
                Object.freeze({ id: "win-a", ref: refs.a, rect: { x: 0, y: 0, w: 600, h: 800 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
                Object.freeze({ id: "win-b", ref: refs.b, rect: { x: 600, y: 0, w: 600, h: 800 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
                Object.freeze({ id: "win-c", ref: refs.c, rect: { x: 0, y: 800, w: 600, h: 400 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
            ];
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-a",
                windows: Object.freeze(wins),
                activeRef: refs.a,
                fingerprint: "fp-a,b,c",
                revalidate: () => true,
            };
        };
        fire(mocks, "added");
        runDebounce(mocks);
        const admitIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, admitIndex)["command"] as Record<string, unknown>), { op: "reconcile" });
        mocks.callbacks[admitIndex]?.(
            plannedReply(
                plannerPayload(mocks, admitIndex)["correlation_id"] as string,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );

        // Directional pointer-resize completes its flight too.
        const pointerBefore = mocks.dbusCalls.length;
        adapter.requestPointerResize("win-a", "left", 0);
        assert.equal(mocks.dbusCalls.length, pointerBefore + 1);
        const pointerIndex = mocks.dbusCalls.length - 1;
        assert.notEqual((plannerPayload(mocks, pointerIndex)["command"] as Record<string, unknown>)["op"], "toggle-float");
        mocks.callbacks[pointerIndex]?.(
            plannedReply(
                plannerPayload(mocks, pointerIndex)["correlation_id"] as string,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );

        // None of the automatic, admission, reconcile, or directional routes
        // may call or set floating state, nor dispatch a toggle-float command.
        assert.deepEqual(mocks.floatingCalls, []);
        for (let index = 0; index < mocks.dbusCalls.length; index += 1) {
            assert.notEqual(
                (plannerPayload(mocks, index)["command"] as Record<string, unknown>)["op"],
                "toggle-float",
                `dbus call ${index} must not be a float transition`,
            );
        }
        assert.ok(!mocks.logs.some((line) => line.includes("float-written")));

        // Only the explicit requestFloat route calls setFloating, and only
        // after the validated planned reply carries the float geometry.
        adapter.requestFloat();
        const floatIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, floatIndex)["command"] as Record<string, unknown>), { op: "toggle-float", window: "win-a" });
        mocks.callbacks[floatIndex]?.(
            JSON.stringify({
                v: 1,
                correlation_id: plannerPayload(mocks, floatIndex)["correlation_id"],
                outcome: "planned",
                desired_geometry: [
                    { window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", leaf: "leaf-c", output: "out-1", workspace: "ws-1", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
            }),
        );
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.ok(mocks.logs.some((line) => line.includes("float-written")));
    });

    it("keeps an intentional float above and restores only its project-owned prior state", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let keepAbove = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, resourceClasses: { "win-a": "ghostty" } });
        mocks.keepAboveReadImpl = (target) => target === refs.a ? keepAbove : null;
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const floatPayload = plannerPayload(mocks, 0);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        floating = true;
        assert.deepEqual(mocks.keepAboveToggles, [{ target: refs.a, keepAbove: true }]);

        adapter.requestFloat();
        const unfloatPayload = plannerPayload(mocks, 1);
        mocks.callbacks[1]?.(plannedReply(
            unfloatPayload["correlation_id"] as string,
            [
                { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
            "win-a-leaf",
        ));
        assert.deepEqual(mocks.keepAboveToggles, [
            { target: refs.a, keepAbove: true },
            { target: refs.a, keepAbove: false },
        ]);
    });

    it("uses KWin's normal above transition and restores an initial keep-below state", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let keepAbove = false;
        let keepBelow = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating } });
        mocks.keepAboveReadImpl = () => keepAbove;
        mocks.keepBelowReadImpl = () => keepBelow;
        mocks.keepAboveToggleImpl = (_target, value) => {
            keepAbove = value;
            if (value) {
                keepBelow = false;
            }
            return "invoked";
        };
        mocks.keepBelowToggleImpl = (_target, value) => {
            keepBelow = value;
            if (value) {
                keepAbove = false;
            }
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const floatPayload = plannerPayload(mocks, 0);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        floating = true;
        adapter.requestFloat();
        const unfloatPayload = plannerPayload(mocks, 1);
        mocks.callbacks[1]?.(plannedReply(
            unfloatPayload["correlation_id"] as string,
            [
                { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
            "win-a-leaf",
        ));
        assert.deepEqual(mocks.keepAboveToggles, [{ target: refs.a, keepAbove: true }]);
        assert.deepEqual(mocks.keepBelowToggles, [{ target: refs.a, keepBelow: true }]);
    });

    it("refuses a float when KWin cannot confirm keep-above ownership", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.keepAboveToggleImpl = () => "refused";
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.deepEqual(mocks.floatingCalls, []);
        assert.ok(mocks.geometries.every((entry) => entry.target !== refs.a));
        assert.ok(mocks.logs.includes("omnitiler:plan:keep-above window=win-a resource_class=unknown target=above outcome=refused"));
    });

    it("restores keep-above if float geometry cannot be written", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let keepAbove = false;
        mocks.keepAboveReadImpl = () => keepAbove;
        mocks.keepAboveToggleImpl = (_target, value) => {
            keepAbove = value;
            return "invoked";
        };
        mocks.geometryImpl = (target) => target !== refs.a;
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.deepEqual(mocks.keepAboveToggles, [
            { target: refs.a, keepAbove: true },
            { target: refs.a, keepAbove: false },
        ]);
        assert.deepEqual(mocks.floatingCalls, []);
    });
});

describe("plan adapter client self-resize reconcile", () => {
    const allocA = { x: 0, y: 0, w: 600, h: 800 };
    const allocB = { x: 600, y: 0, w: 600, h: 800 };
    function baseline(mocks: Mocks, refs: { a: object; b: object; c: object }): PlanAdapter {
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": allocA, "win-b": allocB } });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const corr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(plannedReply(corr, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        return adapter;
    }
    function driftRects(kind: string): Record<string, { x: number; y: number; w: number; h: number }> {
        return kind === "increment"
            ? { "win-a": { x: 0, y: 0, w: 616, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } }
            : { "win-a": { x: 0, y: 0, w: 200, h: 200 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } };
    }
    function reconcileOnce(mocks: Mocks, index: number, drift: Record<string, { x: number; y: number; w: number; h: number }>): void {
        const corr = plannerPayload(mocks, index)["correlation_id"] as string;
        mocks.callbacks[index]?.(plannedReply(corr, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        void drift;
    }
    it("reprojects a scale-style work-area change through retained projection with selected gaps", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        (mocks.env as { isInteractiveResizeActive?: () => boolean }).isInteractiveResizeActive = () => true;
        const writesBeforeScale = mocks.geometries.length;
        const scaled = { x: 0, y: 0, w: 1800, h: 1200 };
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, bounds: scaled, rects: { "win-a": allocA, "win-b": allocB } });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>), { op: "reconcile" });
        assert.deepEqual(plannerPayload(mocks, 1)["domain"], {
            output: "out-1",
            workspace: "ws-1",
            bounds: scaled,
            gap: DOMAIN_GAP,
            outer_gap: OUTER_DOMAIN_GAP,
        });
        const scaledCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:scope-transition old=0,0,1200,800 new=0,0,1800,1200",
            ),
            "dedicated old-bounds to new-bounds scope transition",
        );
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:work-area-reprojection selected=retained"),
            "distinct retained reprojection event, not the generic reconcile line",
        );
        mocks.callbacks[1]?.(
            plannedReply(
                scaledCorrelation,
                [
                    { window: "win-a", rect: { x: 8, y: 8, w: 888, h: 1184 } },
                    { window: "win-b", rect: { x: 904, y: 8, w: 888, h: 1184 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.geometries.slice(writesBeforeScale), [
            { target: refs.a, rect: { x: 8, y: 8, w: 888, h: 1184 } },
            { target: refs.b, rect: { x: 904, y: 8, w: 888, h: 1184 } },
        ]);
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${scaledCorrelation}`) && line.includes("kind=reconcile") && line.includes("outcome=planned-applied")));

        const shrunk = { x: 0, y: 0, w: 800, h: 600 };
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, bounds: shrunk, rects: { "win-a": { x: 0, y: 0, w: 800, h: 900 }, "win-b": { x: 800, y: 0, w: 800, h: 900 } } });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 3);
        const shrinkCorrelation = plannerPayload(mocks, 2)["correlation_id"] as string;
        const carried = plannerPayload(mocks, 2)["windows"] as Array<Record<string, unknown>>;
        for (const entry of carried) {
            const rect = entry["rect"] as { x: number; y: number; w: number; h: number };
            assert.ok(rect.x >= shrunk.x && rect.y >= shrunk.y);
            assert.ok(rect.x + rect.w <= shrunk.x + shrunk.w);
            assert.ok(rect.y + rect.h <= shrunk.y + shrunk.h);
        }
        assert.notDeepEqual(
            carried.map((entry) => entry["rect"]),
            [{ x: 0, y: 0, w: 800, h: 900 }, { x: 800, y: 0, w: 800, h: 900 }],
            "reprojection must not send drifted client rectangles",
        );
        mocks.callbacks[2]?.(rejectedReply(shrinkCorrelation, "snapshot-invalid"));
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 4, "a rejected work-area reprojection does not park");
        const retryCorrelation = plannerPayload(mocks, 3)["correlation_id"] as string;
        const writesBeforeRetry = mocks.geometries.length;
        mocks.callbacks[3]?.(
            plannedReply(
                retryCorrelation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 600 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 600 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.geometries.slice(writesBeforeRetry), [
            { target: refs.a, rect: { x: 0, y: 0, w: 400, h: 600 } },
            { target: refs.b, rect: { x: 400, y: 0, w: 400, h: 600 } },
        ]);
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${retryCorrelation}`) && line.includes("outcome=planned-applied")));
    });
    it("routes an outer-gap change through reconcile, then one update-gaps retry on the exact refusal", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        const callsBefore = mocks.dbusCalls.length;
        const writesBefore = mocks.geometries.length;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                domainOuterGap: 0,
                rects: { "win-a": allocA, "win-b": allocB },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        // The foreground automatic route sends the complete observation as
        // reconcile, never silent adopt; only the correlated exact-message
        // gap refusal retries one update-gaps.
        assert.equal(mocks.dbusCalls.length, callsBefore + 1);
        const cmd = plannerPayload(mocks, callsBefore)["command"] as Record<string, unknown>;
        assert.deepEqual(cmd, { op: "reconcile" });
        assert.equal((plannerPayload(mocks, callsBefore)["domain"] as Record<string, unknown>)["outer_gap"], 0);
        const firstCorr = plannerPayload(mocks, callsBefore)["correlation_id"] as string;
        mocks.callbacks[callsBefore]?.(
            JSON.stringify({
                v: 1,
                correlation_id: firstCorr,
                outcome: "rejected",
                kind: "domain-mismatch",
                message: "domain outer gap does not match retained state",
            }),
        );
        assert.equal(mocks.dbusCalls.length, callsBefore + 2);
        assert.deepEqual((plannerPayload(mocks, callsBefore + 1)["command"] as Record<string, unknown>), {
            op: "update-gaps",
        });
        assert.equal((plannerPayload(mocks, callsBefore + 1)["domain"] as Record<string, unknown>)["outer_gap"], 0);
        // The applied retry reply converges the new gaps natively.
        const corr = plannerPayload(mocks, callsBefore + 1)["correlation_id"] as string;
        mocks.callbacks[callsBefore + 1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 596, h: 800 } },
                    { window: "win-b", rect: { x: 604, y: 0, w: 596, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.geometries.slice(writesBefore), [
            { target: refs.a, rect: { x: 0, y: 0, w: 596, h: 800 } },
            { target: refs.b, rect: { x: 604, y: 0, w: 596, h: 800 } },
        ]);
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${corr}`) && line.includes("kind=update-gaps") && line.includes("outcome=planned-applied")));
    });
    it("retains allocation on self-resize and resets on matching geometry", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        const writesAfterBaseline = mocks.geometries.length;
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        const cmd = plannerPayload(mocks, 1)["command"] as Record<string, unknown>;
        assert.deepEqual(cmd, { op: "reconcile" });
        const wins = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual((wins.find((w) => w["window"] === "win-a") as Record<string, unknown>)["rect"], { x: 0, y: 0, w: 616, h: 800 });
        reconcileOnce(mocks, 1, driftRects("increment"));
        assert.ok(mocks.geometries.length > writesAfterBaseline);
        assert.ok(mocks.logs.some((l) => l.includes("kind=reconcile") && l.includes("outcome=planned-applied")));
        assert.ok(!mocks.logs.some((l) => l.includes("stale-dropped")));
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": allocA, "win-b": allocB } });
        const callsBefore = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(adapter.isEnabled, true);
    });
    it("suppresses a held interactive resize then reconciles once it finishes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        let interactiveResize = false;
        (mocks.env as { isInteractiveResizeActive?: () => boolean }).isInteractiveResizeActive = () => interactiveResize;

        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "pre-start drift opens one ordinary reconcile");
        const writesBeforeStart = mocks.geometries.length;

        interactiveResize = true;
        adapter.setInteractiveResizeActive(true);
        const staleCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(plannedReply(staleCorrelation, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        assert.equal(mocks.geometries.length, writesBeforeStart, "a pre-start reply cannot write into the held resize");

        for (const width of [700, 800, 900]) {
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: width, h: 800 }, "win-b": allocB },
                });
            fire(mocks, "geometry");
            runDebounce(mocks);
        }
        assert.equal(mocks.dbusCalls.length, 2, "held pointer updates dispatch no ordinary reconcile");
        assert.equal(mocks.geometries.length, writesBeforeStart, "held pointer updates write no retained geometry");

        interactiveResize = false;
        adapter.setInteractiveResizeActive(false);
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 3, "finish schedules one normal retained reconcile");
        const finishCorrelation = plannerPayload(mocks, 2)["correlation_id"] as string;
        mocks.callbacks[2]?.(plannedReply(finishCorrelation, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        assert.ok(mocks.geometries.length > writesBeforeStart, "the finish reconcile restores the retained allocation");

        for (let attempt = 0; attempt < 2; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 3 + attempt;
            const correlation = plannerPayload(mocks, index)["correlation_id"] as string;
            mocks.callbacks[index]?.(plannedReply(correlation, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        }
        assert.ok(
            !mocks.logs.some((line) => line === "omnitiler:plan:reconcile-parked"),
            "no domain-wide park remains",
        );
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:reconcile-accepted windows=1 cause=stable-drift recovery=accept-client-rect"),
            "third post-finish drift accepts the exact client rect",
        );
        assert.equal(mocks.dbusCalls.length, 5, "acceptance quiets without dispatching");
    });
    it("lets a pointer resize preempt the queued interactive-finish resync", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        let interactiveResize = true;
        (mocks.env as { isInteractiveResizeActive?: () => boolean }).isInteractiveResizeActive = () => interactiveResize;

        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        adapter.setInteractiveResizeActive(true);
        adapter.setInteractiveResizeActive(false);
        interactiveResize = false;
        assert.equal(adapter.requestPointerResize("win-a", "right", 700), true);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>), {
            op: "pointer-resize",
            window: "win-a",
            direction: "right",
            boundary: 700,
        });
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "the queued finish resync cannot compete with the pointer route");
    });
    it("reasserts allocation across a focus-only change while geometry has drifted", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        const writesAfterBaseline = mocks.geometries.length;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                fingerprint: "fp-2",
                rects: driftRects("increment"),
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        const cmd = plannerPayload(mocks, 1)["command"] as Record<string, unknown>;
        assert.deepEqual(cmd, { op: "reconcile" });
        const wins = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual((wins.find((w) => w["window"] === "win-a") as Record<string, unknown>)["rect"], { x: 0, y: 0, w: 616, h: 800 });
        reconcileOnce(mocks, 1, driftRects("increment"));
        assert.ok(mocks.geometries.length > writesAfterBaseline);
        assert.ok(mocks.logs.some((l) => l.includes("kind=reconcile") && l.includes("outcome=planned-applied")));
        assert.ok(!mocks.logs.some((l) => l.includes("stale-dropped")));
        assert.equal(adapter.isEnabled, true);
    });
    it("reasserts allocation across a fingerprint-only change while geometry has drifted", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        const writesAfterBaseline = mocks.geometries.length;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                fingerprint: "fp-2",
                rects: driftRects("increment"),
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        reconcileOnce(mocks, 1, driftRects("increment"));
        assert.ok(mocks.geometries.length > writesAfterBaseline);
        assert.ok(mocks.logs.some((l) => l.includes("kind=reconcile") && l.includes("outcome=planned-applied")));
        assert.equal(adapter.isEnabled, true);
    });
    it("accepts stable drift after three writes without oscillation", () => {
        for (const kind of ["increment", "minimum"]) {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            baseline(mocks, refs);
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects(kind) });
            for (let attempt = 0; attempt < 3; attempt += 1) {
                fire(mocks, "geometry");
                runDebounce(mocks);
                const index = 1 + attempt;
                assert.equal(mocks.dbusCalls.length, index + 1);
                assert.deepEqual((plannerPayload(mocks, index)["command"] as Record<string, unknown>)["op"], "reconcile");
                reconcileOnce(mocks, index, driftRects(kind));
                assert.ok(mocks.logs.some((l) => l.includes(`cmd=gen-1-p${String(index)}`) && l.includes("outcome=planned-applied")));
            }
            assert.ok(
                !mocks.logs.some((line) => line === "omnitiler:plan:reconcile-parked"),
                "no domain-wide park remains",
            );
            const acceptedBefore = mocks.dbusCalls.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.ok(
                mocks.logs.some((line) => line === "omnitiler:plan:reconcile-accepted windows=1 cause=stable-drift recovery=accept-client-rect"),
                "bounded acceptance transition token",
            );
            assert.equal(mocks.dbusCalls.length, acceptedBefore, "acceptance quiets without dispatching");
            const acceptedWrites = mocks.geometries.length;
            const acceptedLogs = mocks.logs.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, acceptedBefore, "stable accepted drift stays quiet");
            assert.equal(mocks.geometries.length, acceptedWrites);
            const quietNew = mocks.logs.slice(acceptedLogs);
            assert.ok(
                quietNew.every((line) =>
                    line.startsWith("omnitiler:route-diag component=cosmic-plan route=plan stage=refresh event=foreground outcome=equal"),
                ),
                "quiet accepted drift logs only equal refresh classifications",
            );
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": allocA, "win-b": allocB } });
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, acceptedBefore + 1, "changed client rect resumes ordinary reconcile");
            assert.deepEqual((plannerPayload(mocks, acceptedBefore)["command"] as Record<string, unknown>)["op"], "reconcile");
        }
    });
    it("per-window acceptance lets other drift keep reconciling while stable stays quiet", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        void adapter;
        const driftA = { x: 0, y: 0, w: 616, h: 800 };
        const driftB = { x: 600, y: 0, w: 500, h: 800 };
        const both = { "win-a": driftA, "win-b": driftB };
        for (let attempt = 0; attempt < 3; attempt += 1) {
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: both });
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 1 + attempt;
            reconcileOnce(mocks, index, both);
        }
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: both });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:reconcile-accepted windows=2 cause=stable-drift recovery=accept-client-rect"),
            "two-window stable drift accepts per-window",
        );
        const acceptedCalls = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, acceptedCalls, "stable pair stays quiet");
        const movedB = { x: 600, y: 0, w: 550, h: 800 };
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": driftA, "win-b": movedB } });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, acceptedCalls + 1, "moving window resumes ordinary reconcile");
        assert.deepEqual((plannerPayload(mocks, acceptedCalls)["command"] as Record<string, unknown>)["op"], "reconcile");
        const sent = plannerPayload(mocks, acceptedCalls)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            (sent.find((entry) => entry["window"] === "win-a") as Record<string, unknown>)["rect"],
            driftA,
            "stable window carries its accepted rect",
        );
        assert.deepEqual(
            (sent.find((entry) => entry["window"] === "win-b") as Record<string, unknown>)["rect"],
            movedB,
            "moving window carries its exact client rect",
        );
        assert.ok(
            mocks.logs
                .filter((line) => line.includes("reconcile-accepted"))
                .every((line) => !line.includes("win-a") && !line.includes("win-b") && !line.includes("[object")),
            "acceptance logs carry no window identity",
        );
    });
    it("explicit commands still dispatch after per-window acceptance", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        const drift = driftRects("increment");
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: drift });
        for (let attempt = 0; attempt < 3; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            reconcileOnce(mocks, 1 + attempt, drift);
        }
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.ok(mocks.logs.some((line) => line.includes("reconcile-accepted")), "drift accepted");
        const acceptedCalls = mocks.dbusCalls.length;
        adapter.requestMove("left");
        assert.equal(mocks.dbusCalls.length, acceptedCalls + 1, "explicit move never blocked");
        const moveCorrelation = plannerPayload(mocks, acceptedCalls)["correlation_id"] as string;
        mocks.callbacks[acceptedCalls]?.(
            plannedReply(moveCorrelation, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"),
        );
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: drift });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, acceptedCalls + 2, "post-command drift reconciles normally");
    });
    it("reconcile rejected and planned paths emit one precise reason", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        fire(mocks, "geometry");
        runDebounce(mocks);
        const corr = plannerPayload(mocks, 1)["correlation_id"] as string;
        const logsBefore = mocks.logs.length;
        mocks.callbacks[1]?.(rejectedReply(corr, "snapshot-invalid"));
        const freshLogs = mocks.logs.slice(logsBefore);
        assert.equal(freshLogs.length, 5);
        assert.equal(freshLogs[0], `omnitiler:plan:cmd=${corr} kind=reconcile windows=2 component=cosmic-plan route=plan stage=reply correlation=${corr} generation=gen-1 revision=0 event=reply outcome=received cause=-`);
        assert.equal(freshLogs[1], `omnitiler:plan:cmd=${corr} kind=reconcile windows=2 component=cosmic-plan route=plan stage=reply correlation=${corr} generation=gen-1 revision=0 event=validate outcome=rejected cause=snapshot-invalid`);
        assert.equal(freshLogs[2], `omnitiler:plan:cmd=${corr} kind=reconcile windows=2 component=cosmic-plan route=plan stage=terminal correlation=${corr} generation=gen-1 revision=0 event=settled outcome=rejected cause=validate`);
        assert.ok(freshLogs[3]?.includes("kind=reconcile") && freshLogs[3]?.includes("outcome=rejected"));
        assert.equal(freshLogs[4], "omnitiler:plan:rejected kind=snapshot-invalid");
        fire(mocks, "geometry");
        runDebounce(mocks);
        const corr2 = plannerPayload(mocks, 2)["correlation_id"] as string;
        const logsBefore2 = mocks.logs.length;
        mocks.callbacks[2]?.(plannedReply(corr2, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        const fresh2 = mocks.logs.slice(logsBefore2);
        assert.equal(fresh2.length, 9);
        assert.equal(fresh2[0], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=reply correlation=${corr2} generation=gen-1 revision=0 event=reply outcome=received cause=-`);
        assert.equal(fresh2[1], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=reply correlation=${corr2} generation=gen-1 revision=2 event=validate outcome=validated cause=-`);
        assert.equal(fresh2[2], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=observe correlation=${corr2} generation=gen-1 revision=2 event=observe outcome=matched cause=-`);
        assert.equal(fresh2[3], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=apply correlation=${corr2} generation=gen-1 revision=2 event=apply outcome=started cause=-`);
        assert.equal(fresh2[4], "omnitiler:plan:write window=win-b resource_class=unknown disposition=skip-already-equal rect=600,0,600,800");
        assert.equal(fresh2[5], "omnitiler:plan:write window=win-a resource_class=unknown disposition=written rect=0,0,600,800");
        assert.equal(fresh2[6], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=apply correlation=${corr2} generation=gen-1 revision=2 event=setters outcome=applied cause=skipped-equal`);
        assert.equal(fresh2[7], `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=terminal correlation=${corr2} generation=gen-1 revision=2 event=settled outcome=applied cause=setters`);
        assert.ok(fresh2[8]?.includes("kind=reconcile") && fresh2[8]?.includes("outcome=planned-applied"));
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line ===
                    `omnitiler:plan:cmd=${corr2} kind=reconcile windows=2 component=cosmic-plan route=plan stage=request correlation=${corr2} generation=gen-1 revision=0 event=dispatch outcome=started cause=-`,
            ),
            "route entry line for the dispatched reconcile",
        );
    });
    it("accepts repeated rejected drift after three without further D-Bus for stable rects", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        for (let attempt = 0; attempt < 3; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 1 + attempt;
            assert.equal(mocks.dbusCalls.length, index + 1);
            const corr = plannerPayload(mocks, index)["correlation_id"] as string;
            mocks.callbacks[index]?.(rejectedReply(corr, "snapshot-invalid"));
        }
        const acceptedCalls = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, acceptedCalls, "stable rejected drift accepts without dispatching");
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:reconcile-accepted windows=1 cause=stable-drift recovery=accept-client-rect"),
            "bounded acceptance token for stable rejected drift",
        );
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, acceptedCalls, "accepted drift stays quiet");
    });
    it("allows an explicit move after drift acceptance and reconciles afterwards", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baseline(mocks, refs);
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("increment") });
        for (let attempt = 0; attempt < 3; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 1 + attempt;
            mocks.callbacks[index]?.(rejectedReply(plannerPayload(mocks, index)["correlation_id"] as string, "snapshot-invalid"));
        }
        const parkedCalls = mocks.dbusCalls.length;
        adapter.requestMove("left");
        assert.equal(mocks.dbusCalls.length, parkedCalls + 1);
        const moveIndex = parkedCalls;
        const moveCorrelation = plannerPayload(mocks, moveIndex)["correlation_id"] as string;
        assert.deepEqual((plannerPayload(mocks, moveIndex)["command"] as Record<string, unknown>)["op"], "move");
        mocks.callbacks[moveIndex]?.(
            plannedReply(
                moveCorrelation,
                [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }],
                "win-a-leaf",
            ),
        );
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, parkedCalls + 2);
        assert.deepEqual((plannerPayload(mocks, parkedCalls + 1)["command"] as Record<string, unknown>)["op"], "reconcile");
    });
    it("accepts repeated stale drift after three with no further D-Bus for stable rects", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        const drift = driftRects("increment");
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: drift });
        for (let attempt = 0; attempt < 3; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 1 + attempt;
            const corr = plannerPayload(mocks, index)["correlation_id"] as string;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: driftRects("minimum") });
            mocks.callbacks[index]?.(plannedReply(corr, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], null));
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: drift });
        }
        const parkedCalls = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, parkedCalls);
    });
    it("clears drift acceptance counters when a deferred work-area reprojection supersedes a flight", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baseline(mocks, refs);
        const oldDrift = driftRects("increment");
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: oldDrift });
        for (let attempt = 0; attempt < 2; attempt += 1) {
            fire(mocks, "geometry");
            runDebounce(mocks);
            const index = 1 + attempt;
            const correlation = plannerPayload(mocks, index)["correlation_id"] as string;
            mocks.callbacks[index]?.(rejectedReply(correlation, "snapshot-invalid"));
        }

        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 4);
        const inFlightCorrelation = plannerPayload(mocks, 3)["correlation_id"] as string;

        const translated = { x: 200, y: 100, w: 800, h: 600 };
        const translatedDrift = {
            "win-a": { x: 200, y: 100, w: 200, h: 300 },
            "win-b": { x: 600, y: 100, w: 200, h: 300 },
        };
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, bounds: translated, rects: translatedDrift });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 4, "work-area reprojection waits behind the flight");

        mocks.callbacks[3]?.(
            plannedReply(
                inFlightCorrelation,
                [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }],
                "win-a-leaf",
            ),
        );
        assert.equal(mocks.dbusCalls.length, 5);
        const reprojection = plannerPayload(mocks, 4);
        assert.deepEqual(reprojection["command"], { op: "reconcile" });
        const carried = reprojection["windows"] as Array<Record<string, unknown>>;
        for (const entry of carried) {
            const rect = entry["rect"] as { x: number; y: number; w: number; h: number };
            assert.ok(rect.x >= translated.x && rect.y >= translated.y);
            assert.ok(rect.x + rect.w <= translated.x + translated.w);
            assert.ok(rect.y + rect.h <= translated.y + translated.h);
        }
        const reprojectionCorrelation = reprojection["correlation_id"] as string;
        mocks.callbacks[4]?.(
            plannedReply(
                reprojectionCorrelation,
                [
                    { window: "win-a", rect: { x: 208, y: 108, w: 384, h: 584 } },
                    { window: "win-b", rect: { x: 608, y: 108, w: 384, h: 584 } },
                ],
                "win-a-leaf",
            ),
        );

        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                bounds: translated,
                rects: {
                    "win-a": { x: 200, y: 100, w: 100, h: 100 },
                    "win-b": { x: 600, y: 100, w: 200, h: 200 },
                },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 6, "a later drift is not parked by the old flight");
        assert.deepEqual((plannerPayload(mocks, 5)["command"] as Record<string, unknown>)["op"], "reconcile");
    });
});

describe("plan adapter bounded diagnostics", () => {
    it("emits only the bounded plan line shapes with exact tokens and no owner echo", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        mocks.callbacks[0]?.(rejectedReply(plannerPayload(mocks, 0)["correlation_id"] as string, "snapshot-invalid"));
        adapter.requestMove("right");
        const correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                null,
            ),
        );
        assert.ok(mocks.logs.length >= 7);
        for (const line of mocks.logs) {
            assert.match(
                line,
                /^omnitiler:(?:plan:(cmd=\S+ kind=(admit|remove|move|focus|resize|reconcile|pointer-resize) windows=\d+ outcome=\S+|cmd=(gen-1-p\d+) kind=(focus|move) windows=2 component=cosmic-plan route=plan stage=(request correlation=\3 generation=gen-1 revision=0 event=dispatch outcome=started|activate correlation=\3 generation=gen-1 revision=0 event=(presence outcome=(presence-requested|present)|resolve outcome=(resolve-requested|owner-pinned)|send outcome=(send-requested|request-sent))) cause=-|cmd=(gen-1-p\d+) kind=(focus|move) windows=2 component=cosmic-plan route=plan stage=(?:reply correlation=\10 generation=gen-1 revision=0 event=reply outcome=received cause=-|reply correlation=\10 generation=gen-1 revision=(?:0|2|unavailable) event=validate outcome=validated cause=-|reply correlation=\10 generation=gen-1 revision=0 event=validate outcome=rejected cause=snapshot-invalid|reply correlation=\10 generation=gen-1 revision=0 event=validate outcome=(?:malformed|stale) cause=(?:service-fault|precondition-mismatch|correlation-mismatch|stale-dropped)|observe correlation=\10 generation=gen-1 revision=(?:2|unavailable) event=observe outcome=(?:matched cause=-|mismatched cause=stale-scope)|apply correlation=\10 generation=gen-1 revision=(?:2|unavailable) event=apply outcome=started cause=-|apply correlation=\10 generation=gen-1 revision=(?:2|unavailable) event=setters outcome=applied cause=(?:-|skipped-fullscreen|skipped-maximized|skipped-floating|skipped-overconstrained|skipped-clamped|skipped-equal|skipped-mixed)|apply correlation=\10 generation=gen-1 revision=(?:2|unavailable) event=setters outcome=write-failed cause=(?:write-failed|precondition-mismatch)|terminal correlation=\10 generation=gen-1 revision=(?:0|2|unavailable) event=settled outcome=(?:applied cause=(?:setters|apply)|rejected cause=validate|timeout cause=reply|uncertain cause=(?:validate|observe|apply|setters)))|rejected kind=[a-z-]+( detail=[a-z-]+)?|write window=\S+ resource_class=\S+ disposition=(written|skip-fullscreen|skip-maximized|skip-floating|skip-overconstrained|skip-clamped|skip-already-equal|write-failed|float-written|float-write-failed) rect=[^ ]+|busy-refused kind=(focus|move|resize)|(focus|move|resize|pointer)-refused-[a-z-]+|maximize-refused-signal|scope-transition [^ ]+|work-area-reprojection selected=retained|echo-fence-(armed|consumed|cleared-equality|mismatched)|reconcile-parked|clamp-accepted correlation=\S+ window=\S+ resource_class=\S+ op=[a-z-]+|overconstrained-skipped correlation=\S+ window=\S+ resource_class=\S+ op=[a-z-]+|minimum-placed correlation=\S+ window=\S+ resource_class=\S+ op=[a-z-]+ rect=\S+|membership-skew correlation=\S+ op=[a-z-]+ reason=[a-z-]+ wanted=\d+ planned=(\d+|unknown) missing=(\d+|unknown) extra=(\d+|unknown) floating=\d+ sticky=\d+ fullscreen=\d+ maximized=\d+ retained=(known|unknown) retained-wanted=(\d+|-) retained-ids=\S+|membership-skew-member correlation=\S+ window=\S+ side=(missing|extra) floating=(true|false|unknown) sticky=(true|false|unknown) fullscreen=(true|false|unknown) maximized=(true|false|unknown) float-src=(float-set|all-desktops|none|unknown))|route-diag component=cosmic-plan route=plan stage=refresh event=(?:foreground|hidden) outcome=(?:equal|change|uncertain) reason=[a-z-]+ terminal=[a-z-]+ correlation=\S+ generation=\S+)$/,
                line,
            );
            assert.ok(!line.includes("owner-1"), line);
        }
    });

    it("emits an exact refusal token for every distinct request-route cause", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);

        adapter.disable();
        adapter.requestFocus("left");
        adapter.requestMove("right");
        adapter.requestResize("left", "outwards");
        adapter.requestPointerResize("win-a", "right", 1000);
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:focus-refused-disabled"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:move-refused-disabled"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:resize-refused-disabled"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-disabled"));

        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        adapter.requestFocus("sideways");
        adapter.requestMove("sideways");
        adapter.requestResize("sideways", "outwards");
        adapter.requestResize("left", "sideways-mode");
        adapter.requestPointerResize("win-a", "sideways", 1000);
        adapter.requestPointerResize("win-a", "right", 999999);
        adapter.requestPointerResize("bad-id-!@#", "right", 1000);
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:focus-refused-invalid-direction"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:move-refused-invalid-direction"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:resize-refused-invalid-direction"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:resize-refused-invalid-mode"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-direction"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-boundary"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-identity"));

        mocks.observeImpl = () => null;
        adapter.requestFocus("left");
        adapter.requestMove("right");
        adapter.requestResize("left", "outwards");
        adapter.requestPointerResize("win-a", "right", 1000);
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:focus-refused-observe"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:move-refused-observe"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:resize-refused-observe"));
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-observe"));

        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a });
        adapter.requestPointerResize("win-zzz", "right", 1000);
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-absent"));

        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
            });
        adapter.requestPointerResize("win-b", "left", 600);
        assert.ok(mocks.logs.some((l) => l === "omnitiler:plan:pointer-refused-fullscreen"));
    });

    it("dispatches a corner as one dual-axis intent and refuses malformed pairs exactly", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);

        assert.equal(adapter.requestPointerResize("win-a", "right", 700, "down", 900), true);
        assert.equal(mocks.dbusCalls.length, 1, "corner is one request, never two");
        const command = (JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.deepEqual(command, {
            op: "pointer-resize",
            window: "win-a",
            direction: "right",
            boundary: 700,
            direction2: "down",
            boundary2: 900,
        });

        const callsBefore = mocks.dbusCalls.length;
        assert.equal(adapter.requestPointerResize("win-a", "right", 700, undefined, 900), false);
        assert.equal(adapter.requestPointerResize("win-a", "right", 700, "sideways", 900), false);
        assert.equal(adapter.requestPointerResize("win-a", "right", 700, "left", 900), false);
        assert.equal(adapter.requestPointerResize("win-a", "right", 700, "down", 999999), false);
        assert.equal(adapter.requestPointerResize("win-a", "right", 700, "down", undefined), false);
        assert.equal(mocks.dbusCalls.length, callsBefore, "refused corners dispatch nothing");
        const directionRefusals = mocks.logs.filter((l) => l === "omnitiler:plan:pointer-refused-direction");
        assert.equal(directionRefusals.length, 3, "missing direction2, bad direction2, and same-axis pair bind the direction refusal");
        const boundaryRefusals = mocks.logs.filter((l) => l === "omnitiler:plan:pointer-refused-boundary");
        assert.equal(boundaryRefusals.length, 2, "out-of-range and missing boundary2 bind the boundary refusal");

        mocks.callbacks[0]?.(
            plannedReply(plannerPayload(mocks, 0)["correlation_id"] as string, [
                { window: "win-a", rect: { x: 0, y: 0, w: 700, h: 800 } },
                { window: "win-b", rect: { x: 700, y: 0, w: 500, h: 800 } },
            ], null),
        );
        assert.equal(adapter.requestPointerResize("win-a", "right", 1000), true);
        const single = (JSON.parse(mocks.dbusCalls[mocks.dbusCalls.length - 1]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.ok(!("direction2" in single) && !("boundary2" in single), "single-axis wire shape carries no second axis");
    });
});

describe("plan adapter sticky and maximize toggles", () => {
    it("maximizes and restores a tiled member through one fenced native write each", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let maximized = false;
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": maximized }, resourceClasses: { "win-a": "firefox" } });
        mocks.maximizeToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            maximized = value;
            fire(mocks, "maximize", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        adapter.requestMaximize();
        assert.deepEqual(mocks.maximizeToggles, [{ target: refs.a, maximized: true }, { target: refs.a, maximized: false }]);
        assert.ok(mocks.logs.includes("omnitiler:plan:maximize-toggle-echo-consumed"));
        assert.equal(mocks.maximizeClears.length, 0, "post-admission explicit maximize never uses admission clearing");
    });

    it("refuses maximize and sticky independently for fullscreen and maximize overlays", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { fullscreen: { "win-a": true }, maximized: { "win-a": true }, resourceClasses: { "win-a": "steam" } });
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        adapter.requestSticky();
        assert.equal(mocks.maximizeToggles.length, 0);
        assert.equal(mocks.desktopToggles.length, 0);
        assert.ok(mocks.logs.includes("omnitiler:plan:maximize-refused-fullscreen window=win-a resource_class=steam"));
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-refused-fullscreen window=win-a resource_class=steam"));
    });

    it("floats a tiled member before sticky and freshly admits it after the fenced unsticky echo", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky }, resourceClasses: { "win-a": "firefox" } });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        assert.deepEqual(floatPayload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }, { target: refs.a, allDesktops: false }]);
        assert.equal(mocks.dbusCalls.length, 2, "unsticky restores prior tiled placement by fresh admission");
        assert.deepEqual(plannerPayload(mocks, 1)["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 0, y: 0, w: 100, h: 100 } });
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"));
    });

    it("keeps sticky floats above, retains above on return to a prior float, and restores before fresh tiled admission", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        let keepAbove = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.keepAboveReadImpl = (target) => target === refs.a ? keepAbove : null;
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);

        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.keepAboveToggles, [{ target: refs.a, keepAbove: true }]);

        adapter.requestSticky();
        assert.deepEqual(mocks.keepAboveToggles, [
            { target: refs.a, keepAbove: true },
            { target: refs.a, keepAbove: false },
        ]);
        assert.equal(mocks.dbusCalls.length, 2, "sticky-off restores before fresh tiled admission");
    });

    it("preserves keep-above when sticky returns to a prior normal float", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let sticky = false;
        let keepAbove = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true }, sticky: { "win-a": sticky } });
        mocks.keepAboveReadImpl = () => keepAbove;
        mocks.keepAboveToggleImpl = (_target, value) => {
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        adapter.requestSticky();
        assert.deepEqual(mocks.keepAboveToggles, []);
    });

    it("keeps sticky-on within its domain with no desktop, focus, or rehome writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // A cross-workspace observation is not a plannable foreground domain:
        // the sticky path fails closed instead of planning across workspaces.
        mocks.observeImpl = () => makeObserved(refs, { workspaces: { "win-b": "ws-2" } });
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-refused-observe"));
        assert.equal(mocks.dbusCalls.length, 0);
        assert.equal(mocks.desktopToggles.length, 0);
        // Same-domain sticky-on for the tiled focused window.
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { sticky: { "win-a": sticky }, resourceClasses: { "win-a": "ghostty" } });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        adapter.requestSticky();
        // Exactly one planner command whose snapshot preserves the dispatch
        // domain, the focused window homing, and the sibling workspace.
        assert.equal(mocks.dbusCalls.length, 1);
        const payload = plannerPayload(mocks, 0);
        assert.deepEqual(payload["command"], { op: "toggle-float", window: "win-a" });
        const domain = payload["domain"] as Record<string, unknown>;
        assert.equal(domain["output"], "out-1");
        assert.equal(domain["workspace"], "ws-1");
        assert.equal(payload["focused_window"], "win-a");
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        const byWindow = new Map(windows.map((entry) => [entry["window"], entry]));
        assert.deepEqual([byWindow.get("win-a")?.["output"], byWindow.get("win-a")?.["workspace"]], ["out-1", "ws-1"]);
        assert.deepEqual([byWindow.get("win-b")?.["output"], byWindow.get("win-b")?.["workspace"]], ["out-1", "ws-1"]);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 100, h: 100 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        // Only the intended transitions: float placement plus setAllDesktops.
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        assert.equal(mocks.geometries.length, 1, "no sibling geometry move occurs");
        assert.deepEqual(mocks.geometries[0], { target: refs.a, rect: { x: 240, y: 160, w: 720, h: 480 } });
        assert.equal(mocks.actives.length, 0, "sticky-on changes no focus");
        assert.equal(mocks.desktopsWrites.length, 0, "sticky-on writes no desktop membership");
        assert.equal(mocks.dbusCalls.length, 1, "sticky-on issues no follow-up planner commands");
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"));
    });

    it("keeps an already floating sticky member floating and makes a deliberate maximize block float", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = true;
        let sticky = false;
        let maximized = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky }, maximized: { "win-a": maximized }, resourceClasses: { "win-a": "firefox" } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        mocks.maximizeToggleImpl = (_target, value) => {
            maximized = value;
            fire(mocks, "maximize", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        adapter.requestSticky();
        assert.equal(mocks.dbusCalls.length, 0, "prior floating sticky never enters the tile tree");
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }, { target: refs.a, allDesktops: false }]);
        floating = false;
        adapter.requestMaximize();
        adapter.requestFloat();
        assert.ok(mocks.logs.includes("omnitiler:plan:float-refused-maximize"));
    });

    it("clears a native-state fence without retry when its setter emits no echo", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        adapter.requestMaximize();
        assert.equal(mocks.maximizeToggles.length, 2, "each discrete activation issues one native write");
        assert.ok(mocks.logs.includes("omnitiler:plan:maximize-toggle-echo-cleared-no-signal"));
        assert.ok(!mocks.logs.some((line) => line.includes("maximize-refused-attempted")), "per-activation semantics never refuse an attempted repeat");
    });

    it("issues one sticky write per discrete activation and never infers from incomplete observations", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true } });
        mocks.keepAboveReadImpl = () => true;
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, 1);
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, 2, "a discrete repeat issues again with no persistent suppression");
        assert.ok(!mocks.logs.some((line) => line.includes("sticky-refused-attempted")), "per-activation semantics never refuse an attempted repeat");
        const savedObserve = mocks.observeImpl;
        mocks.observeImpl = () => null;
        fire(mocks, "geometry");
        runDebounce(mocks);
        mocks.observeImpl = savedObserve;
        const beforeIncomplete = mocks.desktopToggles.length;
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, beforeIncomplete + 1, "a complete observation after an incomplete round still issues");
        mocks.observeImpl = () => null;
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, beforeIncomplete + 1, "incomplete observation still refuses without a write");
        assert.ok(mocks.logs.some((line) => line.includes("sticky-refused-observe")));
        mocks.observeImpl = savedObserve;
        fire(mocks, "removed", refs.b);
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, beforeIncomplete + 2, "unrelated removal never suppresses the next activation");
        fire(mocks, "removed", refs.a);
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, beforeIncomplete + 3, "exact removed object never suppresses the next activation");
    });

    it("a failed maximize write allows one attempt on a later identical activation", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let fail = true;
        mocks.maximizeToggleImpl = () => (fail ? "missing" : "invoked");
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        assert.equal(mocks.maximizeToggles.length, 1);
        const retryLine = mocks.logs.find((line) => line.includes("omnitiler:plan:maximize-retry-armed"));
        assert.ok(retryLine !== undefined, "failed write logs bounded retry recovery");
        assert.ok(retryLine.includes("window=win-a"));
        assert.ok(retryLine.includes("cause=native-write-missing"));
        assert.ok(retryLine.includes("recovery=retry-on-next-press"));
        fail = false;
        adapter.requestMaximize();
        assert.equal(mocks.maximizeToggles.length, 2, "identical repeat retries after a non-invoked write");
        assert.deepEqual(mocks.maximizeToggles[1], { target: refs.a, maximized: true });
    });

    it("reissues an identical maximize after a same-ref native restore", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let maximized = false;
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": maximized } });
        mocks.maximizeToggleImpl = (_target, value) => {
            maximized = value;
            fire(mocks, "maximize", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        assert.deepEqual(mocks.maximizeToggles, [{ target: refs.a, maximized: true }]);
        maximized = false;
        fire(mocks, "maximize", refs.a);
        adapter.requestMaximize();
        assert.deepEqual(mocks.maximizeToggles, [
            { target: refs.a, maximized: true },
            { target: refs.a, maximized: true },
        ]);
    });

    it("reissues an identical restore after a same-ref native maximize", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let maximized = true;
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": maximized } });
        mocks.maximizeToggleImpl = (_target, value) => {
            maximized = value;
            fire(mocks, "maximize", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        assert.deepEqual(mocks.maximizeToggles, [{ target: refs.a, maximized: false }]);
        maximized = true;
        fire(mocks, "maximize", refs.a);
        adapter.requestMaximize();
        assert.deepEqual(mocks.maximizeToggles, [
            { target: refs.a, maximized: false },
            { target: refs.a, maximized: false },
        ]);
    });

    it("reissues sticky-on from a tiled origin after a same-ref native unstick", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        assert.deepEqual(floatPayload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        sticky = false;
        fire(mocks, "desktops", refs.a);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [
            { target: refs.a, allDesktops: true },
            { target: refs.a, allDesktops: true },
        ]);
    });

    it("reissues sticky-off from a float origin after a same-ref native restick", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let floating = true;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [
            { target: refs.a, allDesktops: true },
            { target: refs.a, allDesktops: false },
        ]);
        sticky = true;
        fire(mocks, "desktops", refs.a);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [
            { target: refs.a, allDesktops: true },
            { target: refs.a, allDesktops: false },
            { target: refs.a, allDesktops: false },
        ]);
    });

    it("reissues maximize across two same-ref native restore cycles with no suppression", () => {
        // B1 repeat: a second native restore/repress round still issues one write per activation.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let maximized = false;
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": maximized } });
        mocks.maximizeToggleImpl = (_target, value) => {
            maximized = value;
            fire(mocks, "maximize", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestMaximize();
        for (let cycle = 0; cycle < 2; cycle += 1) {
            maximized = false;
            fire(mocks, "maximize", refs.a);
            adapter.requestMaximize();
        }
        assert.deepEqual(mocks.maximizeToggles, [
            { target: refs.a, maximized: true },
            { target: refs.a, maximized: true },
            { target: refs.a, maximized: true },
        ]);
        assert.equal(maximized, true, "two restore cycles end maximized after the third repress");
        assert.ok(!mocks.logs.some((line) => line.includes("maximize-refused-attempted")), "per-activation semantics never refuse a repeated cycle");
    });

    it("reissues sticky-off across two same-ref native restick cycles with no suppression", () => {
        // B2 repeat: a second native restick/repress round still issues one write per activation.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        const floating = true;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        adapter.requestSticky();
        for (let cycle = 0; cycle < 2; cycle += 1) {
            sticky = true;
            fire(mocks, "desktops", refs.a);
            adapter.requestSticky();
        }
        assert.deepEqual(mocks.desktopToggles, [
            { target: refs.a, allDesktops: true },
            { target: refs.a, allDesktops: false },
            { target: refs.a, allDesktops: false },
            { target: refs.a, allDesktops: false },
        ]);
        assert.equal(sticky, false, "two restick cycles end unstuck after the fourth repress");
        assert.ok(!mocks.logs.some((line) => line.includes("sticky-refused-attempted")), "per-activation semantics never refuse a repeated cycle");
    });

    it("stays quiet across duplicate desktopsChanged notifications on a settled sticky float", () => {
        // Duplicate-notification leg: two identical native sticky-state
        // notifications on a settled sticky float dispatch and write nothing.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        assert.equal(floating, true, "sticky-on settles floating");
        assert.equal(sticky, true, "sticky-on settles stuck");
        const callsAfterSettle = mocks.dbusCalls.length;
        const writesAfterSettle = mocks.geometries.length;
        // Converge observation to the applied placement, as real KWin
        // reports after the writes land; duplicates must then stay quiet.
        mocks.observeImpl = () => makeObserved(refs, {
            rects: { "win-a": { x: 100, y: 100, w: 600, h: 400 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            floating: { "win-a": true },
            sticky: { "win-a": true },
        });
        fire(mocks, "desktops", refs.a);
        fire(mocks, "desktops", refs.a);
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterSettle + 1, "duplicate sticky notifications coalesce into one flag reconcile, not one per signal");
        assert.equal(mocks.geometries.length, writesAfterSettle, "duplicate sticky notifications write nothing");
        assert.equal(mocks.desktopToggles.length, 1, "duplicate sticky notifications toggle nothing");
        // The flag reconcile covers only the tiled sibling; the floating
        // member is excluded from reply coverage by contract.
        const reconcile = plannerPayload(mocks, callsAfterSettle)["correlation_id"] as string;
        mocks.callbacks[callsAfterSettle]?.(
            plannedReply(reconcile, [{ window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } }], null),
        );
        fire(mocks, "desktops", refs.a);
        fire(mocks, "desktops", refs.a);
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterSettle + 1, "converged duplicates dispatch nothing further");
        assert.equal(sticky, true, "settled sticky float stays stuck");
        assert.equal(adapter.isEnabled, true);
    });

    it("tiled-origin sticky-on keeps the exact subject active across a synchronous membership steal", () => {
        // Tiled-origin sticky-on under a synchronous membership steal: the
        // all-desktops write flips activation to the sibling while Rust
        // desired_focus names the survivor; the subject keeps focus with one write.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        let keepAbove = false;
        let active: object = refs.a;
        mocks.observeImpl = () => makeObserved(refs, { focused: active, floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.activeImpl = () => active;
        const writableEnv = mocks.env as { setActive: (target: object) => boolean };
        const origSetActive = writableEnv.setActive;
        writableEnv.setActive = (target: object) => {
            const ok = origSetActive(target);
            if (ok) {
                active = target;
            }
            return ok;
        };
        mocks.keepAboveReadImpl = (target) => (target === refs.a ? keepAbove : null);
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            if (allDesktops) {
                active = refs.b;
            }
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        assert.deepEqual(floatPayload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-b-leaf" },
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        assert.equal(active, refs.a, "exact subject remains active across the membership steal");
        assert.deepEqual(mocks.actives, [refs.a], "one synchronous retention write, never a survivor focus write");
    });

    it("option A Meta+G on sticky from tiled origin clears all-desktops, restores keep-above, tiles, and stays tiled", () => {
        // User decision 2026-09-25 option A (tiled origin, trace-essential):
        // sticky-on (Meta+Shift+G) -> Meta+G -> native sticky off -> tile ->
        // next observation stable. Meta+G on the sticky window clears
        // all-desktops via the existing sticky-off path, restores project
        // keep-above, and returns to tiling.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        let keepAbove = false;
        let rects: Record<string, { x: number; y: number; w: number; h: number }> | undefined = undefined;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky }, ...(rects === undefined ? {} : { rects }) });
        mocks.keepAboveReadImpl = (target) => (target === refs.a ? keepAbove : null);
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        // Sticky-on (Meta+Shift+G): tiled member floats first, then takes native all-desktops.
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        assert.deepEqual(floatPayload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }], "sticky stays on until the Meta+G call");
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"));
        // Meta+G on the sticky window clears all-desktops via the existing sticky-off path.
        adapter.requestFloat();
        assert.ok(
            mocks.desktopToggles.some((entry) => entry.target === refs.a && entry.allDesktops === false),
            "Meta+G clears native all-desktops via the existing sticky-off path",
        );
        assert.ok(
            mocks.keepAboveToggles.some((entry) => entry.target === refs.a && entry.keepAbove === false),
            "Meta+G restores project keep-above",
        );
        const tileIndex = mocks.dbusCalls.length - 1;
        assert.ok(tileIndex >= 0, "Meta+G returns to tiling through a fresh toggle-float");
        assert.deepEqual((plannerPayload(mocks, tileIndex)["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.equal((plannerPayload(mocks, tileIndex)["command"] as Record<string, unknown>)["window"], "win-a");
        // Native sticky off plus the tile reply: the next complete observation stays tiled.
        sticky = false;
        floating = false;
        const tileCorrelation = plannerPayload(mocks, tileIndex)["correlation_id"] as string;
        mocks.callbacks[tileIndex]?.(JSON.stringify({
            v: 1,
            correlation_id: tileCorrelation,
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        // The native frames now match the converged tiling, so the next
        // complete observation is exactly the retained baseline.
        rects = { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } };
        const callsAfterTile = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterTile, "next complete observation stays tiled with no further dispatch");
    });

    it("option A Meta+G on sticky from prior ordinary float clears all-desktops, restores keep-above, tiles, and stays tiled", () => {
        // User decision 2026-09-25 option A (prior-float origin, trace-essential):
        // ordinary float (saves pre-float keep-above) -> sticky-on
        // (Meta+Shift+G) -> Meta+G -> native sticky off -> tile -> next
        // observation stable. Meta+G clears all-desktops via the existing
        // sticky-off path, restores project keep-above, and returns to tiling
        // instead of leaving a plain float.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let sticky = false;
        let floating = false;
        let keepAbove = false;
        let rects: Record<string, { x: number; y: number; w: number; h: number }> | undefined = undefined;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky }, ...(rects === undefined ? {} : { rects }), resourceClasses: { "win-a": "ghostty" } });
        mocks.keepAboveReadImpl = (target) => (target === refs.a ? keepAbove : null);
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        // Prior ordinary float first, so the pre-float keep-above is saved by the real float path.
        adapter.requestFloat();
        const floatPayload = plannerPayload(mocks, 0);
        assert.deepEqual(floatPayload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.keepAboveToggles, [{ target: refs.a, keepAbove: true }]);
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        // Sticky-on (Meta+Shift+G) from the ordinary float claims no planner admission.
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }], "sticky stays on until the Meta+G call");
        assert.equal(mocks.dbusCalls.length, 1, "prior-float sticky-on issues no planner command");
        // Meta+G on the sticky window clears all-desktops via the existing sticky-off path.
        adapter.requestFloat();
        assert.ok(
            mocks.desktopToggles.some((entry) => entry.target === refs.a && entry.allDesktops === false),
            "Meta+G clears native all-desktops via the existing sticky-off path",
        );
        assert.deepEqual(mocks.keepAboveToggles, [
            { target: refs.a, keepAbove: true },
            { target: refs.a, keepAbove: false },
        ], "Meta+G restores the saved pre-float keep-above");
        assert.equal(mocks.dbusCalls.length, 2, "Meta+G returns to tiling even from a prior float");
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.equal((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["window"], "win-a");
        // Native sticky off plus the tile reply: the next complete observation stays tiled.
        sticky = false;
        floating = false;
        const tileCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(JSON.stringify({
            v: 1,
            correlation_id: tileCorrelation,
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        // The native frames now match the converged tiling, so the next
        // complete observation is exactly the retained baseline.
        rects = { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } };
        const callsAfterTile = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterTile, "next complete observation stays tiled with no further dispatch");
    });

    it("option A Meta+G on adopted sticky clears all-desktops and tiles with no planner admission for the unstick", () => {
        // User decision 2026-09-25 option A (adopted origin): an eligible
        // window already native-sticky is adopted, then Meta+G clears
        // all-desktops via the existing sticky-off path and returns to tiling.
        // The unstick itself claims no planner admission; only the tiling does.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let sticky = true;
        let floating = true;
        let rects: Record<string, { x: number; y: number; w: number; h: number }> | undefined = {
            "win-a": { x: 100, y: 100, w: 600, h: 400 },
        };
        mocks.observeImpl = () => makeObserved(refs, {
            floating: { "win-a": floating },
            sticky: { "win-a": sticky },
            ...(rects === undefined ? {} : { rects }),
            resourceClasses: { "win-a": "firefox" },
        });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted window=win-a")), "unknown origin is adopted");
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: false }], "Meta+G clears native all-desktops");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }], "adopted unstick marks the normal float before tiling");
        const tileIndex = mocks.dbusCalls.length - 1;
        assert.ok(tileIndex >= 0, "Meta+G returns to tiling from an adopted sticky");
        assert.deepEqual((plannerPayload(mocks, tileIndex)["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.equal((plannerPayload(mocks, tileIndex)["command"] as Record<string, unknown>)["window"], "win-a");
        sticky = false;
        floating = false;
        const tileCorrelation = plannerPayload(mocks, tileIndex)["correlation_id"] as string;
        mocks.callbacks[tileIndex]?.(JSON.stringify({
            v: 1,
            correlation_id: tileCorrelation,
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        rects = { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } };
        const callsAfterTile = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterTile, "next complete observation stays tiled with no further dispatch");
    });

    it("Meta+G on sticky fullscreen and sticky maximized overlays refuses before any native setter", () => {
        // Overlays keep their retained tile as native overlays: Meta+G on a
        // sticky overlay refuses exactly like Meta+Shift+G, with no desktop
        // toggles and no planner dispatch.
        for (const [overlay, token] of [["fullscreen", "sticky-refused-fullscreen"], ["maximized", "sticky-refused-maximize"]] as const) {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const shape = overlay === "fullscreen"
                ? { fullscreen: { "win-a": true }, maximized: {} as Record<string, boolean> }
                : { fullscreen: {} as Record<string, boolean>, maximized: { "win-a": true } };
            mocks.observeImpl = () => makeObserved(refs, {
                ...shape,
                floating: { "win-a": true },
                sticky: { "win-a": true },
                resourceClasses: { "win-a": "steam" },
            });
            const adapter = enableAdapter(mocks);
            adapter.requestFloat();
            assert.equal(mocks.desktopToggles.length, 0, `sticky ${overlay} issues no native all-desktops write`);
            assert.equal(mocks.dbusCalls.length, 0, `sticky ${overlay} dispatches no planner command`);
            assert.ok(
                mocks.logs.includes(`omnitiler:plan:${token} window=win-a resource_class=steam`),
                `sticky ${overlay} refuses with the sticky overlay token`,
            );
        }
    });

    it("Meta+G sticky-off failure preserves sticky with no tiling, and a later explicit Meta+G retries", () => {
        // A refusing native all-desktops setter keeps the sticky window
        // sticky with no planner tiling; the failed origin is retained so one
        // later explicit Meta+G retries once the host still reports sticky.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        let keepAbove = false;
        let fail = true;
        // A foreign active window forces focus retention to run whenever it
        // is invoked, so the failed-write focus assertion below is exact.
        mocks.activeImpl = () => refs.b;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.keepAboveReadImpl = (target) => (target === refs.a ? keepAbove : null);
        mocks.keepAboveToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            keepAbove = value;
            return "invoked";
        };
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            if (fail) {
                return "threw";
            }
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        // Sticky-on succeeds first (tiled member floats, then takes native all-desktops).
        fail = false;
        adapter.requestSticky();
        const floatPayload = plannerPayload(mocks, 0);
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: floatPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        // A refusing native all-desktops setter keeps the sticky window
        // sticky with no planner tiling; the failed origin is retained.
        fail = true;
        const callsBeforeMetaG = mocks.dbusCalls.length;
        const activesBeforeFailed = mocks.actives.length;
        adapter.requestFloat();
        assert.equal(mocks.desktopToggles.length, 2, "failed Meta+G attempt issues exactly one native write with no automatic retry");
        assert.equal(sticky, true, "failed write preserves native sticky");
        assert.equal(mocks.dbusCalls.length, callsBeforeMetaG, "failed write issues no planner tiling");
        assert.equal(mocks.actives.length, activesBeforeFailed, "failed Meta+G writes no focus");
        // Later explicit Meta+G retries while the host still reports sticky, then tiles.
        fail = false;
        adapter.requestFloat();
        assert.deepEqual(mocks.desktopToggles[mocks.desktopToggles.length - 1], { target: refs.a, allDesktops: false });
        const tileIndex = mocks.dbusCalls.length - 1;
        assert.ok(tileIndex >= callsBeforeMetaG, "retry returns to tiling");
        assert.deepEqual((plannerPayload(mocks, tileIndex)["command"] as Record<string, unknown>)["op"], "toggle-float");
        sticky = false;
        floating = false;
        const tileCorrelation = plannerPayload(mocks, tileIndex)["correlation_id"] as string;
        mocks.callbacks[tileIndex]?.(JSON.stringify({
            v: 1,
            correlation_id: tileCorrelation,
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        assert.ok(mocks.keepAboveToggles.some((entry) => entry.target === refs.a && entry.keepAbove === false), "retry restores project keep-above through the shared sticky-off path");
    });
});

describe("plan adapter sticky adoption", () => {
    it("adopts an already-sticky window as prior float: unstick preserves geometry and focus without planner, later float tiles", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let sticky = true;
        let floating = true;
        const rect = { x: 100, y: 100, w: 600, h: 400 };
        mocks.observeImpl = () => makeObserved(refs, {
            floating: { "win-a": floating },
            sticky: { "win-a": sticky },
            rects: { "win-a": rect, "win-b": { x: 0, y: 0, w: 100, h: 100 } },
            resourceClasses: { "win-a": "firefox" },
        });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted window=win-a")), "unknown origin is adopted");
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: false }]);
        assert.equal(mocks.dbusCalls.length, 0, "adopted unstick claims no planner admission");
        assert.equal(mocks.geometries.length, 0, "adopted unstick preserves native geometry");
        assert.equal(mocks.desktopsWrites.length, 0, "adopted unstick switches no workspace");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }], "adopted unstick leaves a normal float");
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"));
        assert.ok(mocks.logs.some((line) => line.includes("sticky-focus-retained window=win-a")));
        sticky = false;
        adapter.requestFloat();
        assert.equal(mocks.dbusCalls.length, 1, "later ordinary float toggle tiles the adopted float");
        assert.deepEqual(plannerPayload(mocks, 0)["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 100, y: 100, w: 600, h: 400 } });
    });

    it("adopts after disable and re-enable without planner admission", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.disable();
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        sticky = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true }, sticky: { "win-a": sticky } });
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted window=win-a")));
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: false }]);
        assert.equal(mocks.dbusCalls.length, 0, "re-enabled unstick claims no planner admission");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
    });

    it("unstick after a current-desktop change keeps geometry and focus with no workspace switch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        const rect = { x: 110, y: 120, w: 640, h: 420 };
        const stickyObserved = (workspace: string): PlanObserved => ({
            domainOutput: "out-1",
            domainWorkspace: workspace,
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: 8,
            domainOuterGap: 8,
            focusedId: "win-a",
            windows: Object.freeze([
                Object.freeze({ id: "win-a", ref: refs.a, rect, output: "out-1", workspace, fullscreen: false, maximized: false, floating: true, sticky: true, resourceClass: "ghostty" }),
                Object.freeze({ id: "win-b", ref: refs.b, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-1", workspace, fullscreen: false, maximized: false, floating: false, sticky: false, resourceClass: "unknown" }),
            ]),
            activeRef: refs.a,
            fingerprint: `fp-${workspace}`,
            revalidate: () => true,
        });
        let workspace = "ws-1";
        let sticky = true;
        mocks.observeImpl = () => stickyObserved(workspace);
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            void sticky;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        workspace = "ws-2";
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted window=win-a")));
        assert.equal(mocks.dbusCalls.length, 0);
        assert.equal(mocks.geometries.length, 0, "desktop change never rewrites geometry");
        assert.equal(mocks.desktopsWrites.length, 0, "native assignment owns the current desktop; no explicit switch");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.ok(mocks.logs.some((line) => line.includes("sticky-focus-retained window=win-a")));
    });

    it("keeps known tiled fresh-admit and known float stays-float behavior", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const tiledPayload = plannerPayload(mocks, 0);
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: tiledPayload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        adapter.requestSticky();
        assert.equal(mocks.dbusCalls.length, 2, "known tiled origin fresh-admits on sticky-off");
        assert.deepEqual(plannerPayload(mocks, 1)["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 0, y: 0, w: 100, h: 100 } });

        const refs2 = makeRefs();
        const mocks2 = mockEnv(refs2);
        let sticky2 = false;
        mocks2.observeImpl = () => makeObserved(refs2, { floating: { "win-a": true }, sticky: { "win-a": sticky2 } });
        mocks2.desktopToggleImpl = (_target, allDesktops) => {
            sticky2 = allDesktops;
            fire(mocks2, "desktops", refs2.a);
            return "invoked";
        };
        const adapter2 = enableAdapter(mocks2);
        adapter2.requestSticky();
        adapter2.requestSticky();
        assert.equal(mocks2.dbusCalls.length, 0, "known float origin never enters the tile tree");
        assert.deepEqual(mocks2.desktopToggles, [{ target: refs2.a, allDesktops: true }, { target: refs2.a, allDesktops: false }]);
    });

    it("a throwing sticky-off keeps canonical tracking and a later retry succeeds without replay", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let sticky = true;
        let fail = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            if (fail) {
                return "threw";
            }
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted window=win-a")));
        assert.ok(mocks.logs.some((line) => line.includes("target=current-desktop") && line.includes("outcome=threw")));
        assert.equal(mocks.floatingCalls.length, 0, "failed unstick fabricates no float tracking");
        assert.equal(mocks.dbusCalls.length, 0, "failed unstick replays no planner command");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.desktopToggles.length, 1, "failed attempt issued exactly one native write with no automatic retry");
        fail = false;
        adapter.requestSticky();
        assert.equal(mocks.desktopToggles.length, 2, "one later explicit retry writes once more");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"));
    });

    it("a stale desktops echo for another window never fabricates unstick", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks.env as { readDesktopIds?: (target: object) => ReadonlyArray<string> | null }).readDesktopIds = () => [];
        let sticky = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true }, sticky: { "win-a": sticky } });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.b);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-mismatched"));
        assert.equal(mocks.floatingCalls.length, 0, "stale echo marks no float");
        assert.equal(mocks.dbusCalls.length, 0, "stale echo dispatches no planner admission");
        assert.equal(sticky, false, "native write itself is not rolled back by the test double");
    });

    it("an untracked sticky window with no desktop-list reader stays refused without writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        assert.equal((mocks.env as { readDesktopIds?: unknown }).readDesktopIds, undefined);
        mocks.observeImpl = () => makeObserved(refs, {
            floating: { "win-a": true },
            sticky: { "win-a": true },
            resourceClasses: { "win-a": "firefox" },
        });
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-refused-untracked window=win-a resource_class=firefox"));
        assert.equal(mocks.desktopToggles.length, 0, "no native desktop write without empty-list proof");
        assert.equal(mocks.dbusCalls.length, 0, "no planner call without adoption");
        assert.equal(mocks.floatingCalls.length, 0, "no float canonical mark without adoption");
        assert.equal(mocks.geometries.length, 0);
    });

    it("adopted sticky still refuses fullscreen, maximize, and missing native identity", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { fullscreen: { "win-a": true }, sticky: { "win-a": true } });
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-refused-fullscreen")));
        assert.equal(mocks.desktopToggles.length, 0);
        mocks.observeImpl = () => makeObserved(refs, { maximized: { "win-a": true }, sticky: { "win-a": true } });
        adapter.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-refused-maximize")));
        assert.equal(mocks.desktopToggles.length, 0);
        mocks.observeImpl = () => null;
        adapter.requestSticky();
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-refused-observe"));
    });

    it("adopts an already-sticky native window at entry startup with an empty desktop list", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as Record<string, unknown>;
        const before = winA["frameGeometry"];
        winA["onAllDesktops"] = true;
        winA["desktops"] = [];
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const dbusBefore = mocks.dbusCalls.length;
        handle?.requestSticky();
        assert.ok(mocks.logs.some((line) => line.includes("sticky-adopted")), "entry startup sticky is adopted");
        assert.equal(winA["onAllDesktops"], false, "sticky-off assigns the current desktop natively");
        assert.deepEqual(winA["frameGeometry"], before, "adopted unstick preserves native placement");
        assert.equal(world.workspace["activeWindow"], winA, "adopted unstick keeps exact focus");
        assert.equal(mocks.dbusCalls.length, dbusBefore, "adopted unstick claims no planner admission");
        winA["desktops"] = [world.desktop];
        handle?.requestFloat();
        assert.equal(mocks.dbusCalls.length, dbusBefore + 1, "later ordinary float toggle tiles the adopted float");
        const payload = JSON.parse(mocks.dbusCalls[dbusBefore]?.payload as string) as Record<string, unknown>;
        assert.deepEqual((payload["command"] as Record<string, unknown>)["op"], "toggle-float");
        handle?.stop();
    });
});

describe("plan adapter float focus retention", () => {
    it("retains the exact toggled window on normal float entry instead of activating the Rust survivor focus", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { resourceClasses: { "win-a": "ghostty" } });
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        assert.deepEqual(payload["command"], { op: "toggle-float", window: "win-a" });
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-b-leaf" },
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.deepEqual(mocks.actives, [], "float entry must not activate the survivor");
        assert.ok(mocks.logs.some((line) => line.includes("float-focus-retained window=win-a")), "retention must be logged");
        assert.equal(mocks.desktopsWrites.length, 0, "float entry writes no desktop membership");
    });

    it("retains the exact toggled window on normal float exit (unfloat)", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = true;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, resourceClasses: { "win-a": "ghostty" } });
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        assert.deepEqual(payload["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 0, y: 0, w: 100, h: 100 } });
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
        }));
        floating = false;
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: false }]);
        assert.deepEqual(mocks.actives, [], "unfloat keeps the toggled window without extra activation");
        assert.ok(mocks.logs.some((line) => line.includes("float-focus-retained window=win-a")));
    });

    it("retains the exact toggled window on tiled-to-sticky entry instead of activating the survivor", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let floating = false;
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": floating }, sticky: { "win-a": sticky }, resourceClasses: { "win-a": "firefox" } });
        mocks.desktopToggleImpl = (target, allDesktops) => {
            assert.equal(target, refs.a);
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        const payload = plannerPayload(mocks, 0);
        assert.deepEqual(payload["command"], { op: "toggle-float", window: "win-a" });
        floating = true;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-b-leaf" },
            float_geometry: { window: "win-a", rect: { x: 100, y: 100, w: 600, h: 400 } },
        }));
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        assert.deepEqual(mocks.actives, [], "sticky entry must not activate the survivor");
        assert.ok(mocks.logs.some((line) => line.includes("float-focus-retained window=win-a")));
        assert.ok(mocks.logs.some((line) => line.includes("sticky-focus-retained window=win-a")));
        assert.equal(mocks.desktopsWrites.length, 0, "sticky entry writes no desktop membership");
        assert.equal(mocks.dbusCalls.length, 1, "sticky entry issues no follow-up planner commands");
    });

    it("retains the exact toggled window across float-to-sticky-to-float native toggles with no planner commands", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let sticky = false;
        mocks.observeImpl = () => makeObserved(refs, { floating: { "win-a": true }, sticky: { "win-a": sticky }, resourceClasses: { "win-a": "firefox" } });
        mocks.desktopToggleImpl = (_target, allDesktops) => {
            sticky = allDesktops;
            fire(mocks, "desktops", refs.a);
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        adapter.requestSticky();
        assert.equal(mocks.dbusCalls.length, 0, "prior floating sticky never enters the tile tree");
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }, { target: refs.a, allDesktops: false }]);
        assert.deepEqual(mocks.actives, [], "native sticky toggles keep the exact window without activation");
        assert.equal(mocks.logs.filter((line) => line.includes("sticky-focus-retained window=win-a")).length, 2);
    });

    it("fails a float entry flight when native focus retention fails, without retry or desktop switch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => makeObserved(refs, { resourceClasses: { "win-a": "ghostty" } });
        mocks.activeImpl = () => refs.b;
        (mocks.env as { setActive: (target: object) => boolean }).setActive = (target: object): boolean => {
            mocks.actives.push(target);
            return false;
        };
        const adapter = enableAdapter(mocks);
        const timersBefore = mocks.timers.length;
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-b-leaf" },
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.deepEqual(mocks.actives, [refs.a], "exactly one bounded retention attempt on the toggled window");
        assert.ok(mocks.logs.some((line) => line.includes("float-focus-failed window=win-a")));
        assert.ok(mocks.logs.some((line) => line.includes("outcome=write-failed")), "flight fails write-failed");
        assert.equal(mocks.desktopsWrites.length, 0, "no desktop membership write on focus failure");
        assert.equal(mocks.timers.length, timersBefore + 2, "dispatch timeout plus the single bounded probe deadline only");
        assert.ok(mocks.timers.filter((timer) => !timer.cancelled).length <= 1, "no live retry timer beyond the single probe deadline");
        assert.equal(adapter.isInFlight, false, "flight is terminal");
    });

    it("retires a throwing float geometry write, restores normal stacking, and accepts a later command", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let keepAbove = false;
        let throwGeometry = true;
        mocks.keepAboveReadImpl = () => keepAbove;
        mocks.keepAboveToggleImpl = (_target, value) => {
            keepAbove = value;
            return "invoked";
        };
        mocks.geometryImpl = (target) => {
            if (target === refs.a && throwGeometry) {
                throw new Error("native geometry write threw");
            }
            return true;
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const first = plannerPayload(mocks, 0);
        const firstReply = JSON.stringify({
            v: 1,
            correlation_id: first["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        });
        assert.doesNotThrow(() => mocks.callbacks[0]?.(firstReply));
        assert.deepEqual(mocks.keepAboveToggles, [
            { target: refs.a, keepAbove: true },
            { target: refs.a, keepAbove: false },
        ]);
        assert.equal(keepAbove, false, "normal preimage is restored after the thrown write");
        assert.equal(adapter.isInFlight, false, "failed write releases the flight fence");
        assert.ok(mocks.timers.every((timer) => timer.cancelled), "failed write cancels the flight deadline");
        assert.ok(mocks.logs.some((line) => line.includes("disposition=float-write-failed")));
        assert.ok(mocks.logs.some((line) => line.includes("outcome=write-failed")));
        const writesAfterFailure = mocks.geometries.length;
        mocks.callbacks[0]?.(firstReply);
        assert.equal(mocks.geometries.length, writesAfterFailure, "stale callback cannot apply a later write");

        throwGeometry = false;
        adapter.requestFloat();
        const second = plannerPayload(mocks, 1);
        mocks.callbacks[1]?.(JSON.stringify({
            v: 1,
            correlation_id: second["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.equal(adapter.isInFlight, false, "later valid command settles normally");
        assert.deepEqual(mocks.floatingCalls, [{ id: "win-a", floating: true }]);
    });

    it("restores a keep-below stacking preimage after a throwing float geometry write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let keepAbove = false;
        let keepBelow = true;
        mocks.keepAboveReadImpl = () => keepAbove;
        mocks.keepBelowReadImpl = () => keepBelow;
        mocks.keepAboveToggleImpl = (_target, value) => {
            keepAbove = value;
            if (value) {
                keepBelow = false;
            }
            return "invoked";
        };
        mocks.keepBelowToggleImpl = (_target, value) => {
            keepBelow = value;
            if (value) {
                keepAbove = false;
            }
            return "invoked";
        };
        mocks.geometryImpl = (target) => {
            if (target === refs.a) {
                throw new Error("native geometry write threw");
            }
            return true;
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFloat();
        const payload = plannerPayload(mocks, 0);
        assert.doesNotThrow(() => mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [{ window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        })));
        assert.deepEqual(mocks.keepAboveToggles, [{ target: refs.a, keepAbove: true }]);
        assert.deepEqual(mocks.keepBelowToggles, [{ target: refs.a, keepBelow: true }]);
        assert.equal(keepAbove, false);
        assert.equal(keepBelow, true, "keep-below preimage is restored after the thrown write");
        assert.equal(adapter.isInFlight, false);
    });

    it("logs sticky-focus-stale without activating when the toggled window disappears", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let calls = 0;
        const full = (): PlanObserved => makeObserved(refs, { floating: { "win-a": true }, resourceClasses: { "win-a": "firefox" } });
        mocks.observeImpl = (): PlanObserved | null => {
            calls += 1;
            if (calls === 1) {
                return full();
            }
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-b",
                windows: Object.freeze([
                    Object.freeze({ id: "win-b", ref: refs.b, rect: { x: 600, y: 0, w: 600, h: 800 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false, floating: false, sticky: false, resourceClass: "unknown" }),
                ]),
                activeRef: refs.b,
                fingerprint: "fp-stale",
                revalidate: () => true,
            };
        };
        mocks.desktopToggleImpl = (_target, _allDesktops) => "invoked";
        const adapter = enableAdapter(mocks);
        adapter.requestSticky();
        assert.deepEqual(mocks.desktopToggles, [{ target: refs.a, allDesktops: true }]);
        assert.deepEqual(mocks.actives, [], "stale toggled window is never activated");
        assert.ok(mocks.logs.some((line) => line.includes("sticky-focus-stale window=win-a")));
    });
});

describe("plan adapter fullscreen toggle", () => {
    it("toggles the focused window fullscreen on and back off with one native write each", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let fullscreen = false;
        mocks.observeImpl = () => makeObserved(refs, { fullscreen: { "win-a": fullscreen }, resourceClasses: { "win-a": "ghostty" } });
        mocks.fullscreenToggleImpl = (target, value) => {
            assert.equal(target, refs.a);
            fullscreen = value;
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        adapter.requestFullscreen();
        adapter.requestFullscreen();
        assert.deepEqual(mocks.fullscreenToggles, [{ target: refs.a, fullscreen: true }, { target: refs.a, fullscreen: false }]);
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-toggle window=win-a resource_class=ghostty target=fullscreen outcome=invoked"));
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-toggle window=win-a resource_class=ghostty target=restored outcome=invoked"));
        assert.equal(mocks.dbusCalls.length, 0, "fullscreen toggle issues no planner commands");
        assert.equal(mocks.geometries.length, 0, "fullscreen toggle writes no geometry");
        assert.equal(mocks.actives.length, 0, "fullscreen toggle changes no focus");
        assert.equal(mocks.desktopToggles.length, 0, "fullscreen toggle changes no desktops");
        assert.equal(mocks.floatingCalls.length, 0, "fullscreen toggle changes no float state");
    });

    it("refuses without observation and reports a throwing seam without planner commands", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => null;
        const adapter = enableAdapter(mocks);
        adapter.requestFullscreen();
        assert.equal(mocks.fullscreenToggles.length, 0);
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-refused-observe"));
        mocks.observeImpl = () => makeObserved(refs, { resourceClasses: { "win-a": "firefox" } });
        mocks.fullscreenToggleImpl = (): MaximizeClearOutcome => {
            throw new Error("native write threw");
        };
        adapter.requestFullscreen();
        assert.equal(mocks.fullscreenToggles.length, 1);
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-toggle window=win-a resource_class=firefox target=fullscreen outcome=threw"));
        assert.equal(mocks.dbusCalls.length, 0);
    });
});

describe("plan adapter source hygiene", () => {
    it("performs no tiling, shortcut, config, desktop, outline, or polling access", () => {
        const src = readFileSync(join(kwinSrcDir(), "plan-adapter.ts"), "utf8");
        const entry = readFileSync(join(kwinSrcDir(), "plan-adapter-entry.ts"), "utf8");
        for (const body of [src, entry]) {
            assert.ok(!body.includes("rootTile"), "rootTile");
            assert.ok(!body.includes("relativeGeometry"), "relativeGeometry");
            assert.ok(!body.includes("createDesktop"), "createDesktop");
            assert.ok(!body.includes("removeDesktop"), "removeDesktop");
            assert.ok(!body.includes("showOutline"), "showOutline");
            assert.ok(!body.includes("setTimeout"), "setTimeout");
            assert.ok(!body.includes("setInterval"), "setInterval");
            assert.ok(!body.includes("requestAnimationFrame"), "requestAnimationFrame");
            assert.ok(!body.includes("pollFor"), "pollFor");
        }
        assert.ok(!src.includes("fallback"), "fallback");
        // The entry's only "fallback" is the correlated drag-press-absent
        // diagnostic (absent/stale/other-window press keeps the Started
        // capture); no behavior fallback exists.
        assert.ok(entry.includes("drag-press-fallback"), "press-absent diagnostic");
        assert.equal(entry.split("drag-press-fallback").length - 1, 2, "only the two press-absent log lines");
        assert.equal(entry.split("fallback").length - 1, 2, "no other fallback");
        assert.ok(!src.includes("registerShortcut"), "adapter must not register shortcuts");
        assert.ok(entry.includes("connectSignal"), "entry must use the shared connector");
        assert.ok(entry.includes("signal-capability"), "entry must use the shared layer");
    });

    it("keeps legacy movement, focus, and resize routes out of production startup", () => {
        const entry = readFileSync(join(kwinSrcDir(), "entry.ts"), "utf8");
        assert.ok(!entry.includes("movement-adapter"), "movement-adapter");
        assert.ok(!entry.includes("MovementAdapter"), "MovementAdapter");
        assert.ok(!entry.includes("DescribeMovement"), "DescribeMovement");
        assert.ok(!entry.includes("focus-adapter"), "focus-adapter");
        assert.ok(!entry.includes("FocusAdapter"), "FocusAdapter");
        assert.ok(!entry.includes("DescribeFocus"), "DescribeFocus");
        assert.ok(!entry.includes("resize-adapter"), "resize-adapter");
        assert.ok(!entry.includes("ResizeAdapter"), "ResizeAdapter");
        assert.ok(!entry.includes("DescribeResize"), "DescribeResize");
        assert.ok(!entry.includes("DescribePlan"), "legacy plan route");
        assert.ok(!entry.includes("legacy-engine-removed"), "legacy marker");
        assert.ok(entry.includes("world-entry"), "world entry wiring");
        assert.ok(entry.includes("startWorldAdapterEntry"), "world entry start");
    });
});

// Live-observation entry tests through injected fakes only.

interface FakeSignal {
    readonly handlers: Array<() => void>;
    readonly signal: { connect: (handler: () => void) => void; disconnect: (handler: () => void) => void };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<() => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: () => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: () => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

interface FakeWorld {
    readonly workspace: Record<string, unknown>;
    wins: Array<Record<string, unknown>>;
    readonly output: Record<string, unknown>;
    readonly desktop: Record<string, unknown>;
    readonly added: FakeSignal;
    readonly removed: FakeSignal;
    readonly winFull: Map<object, FakeSignal>;
    readonly winMax: Map<object, FakeSignal>;
    readonly winDesktops: Map<object, FakeSignal>;
    readonly winGeometry: Map<object, FakeSignal>;
    readonly winInteractiveGeometry: Map<object, FakeSignal>;
    readonly maximizeClears: object[];
}

function fakeWorld(): FakeWorld {
    const output: Record<string, unknown> = { name: "out-1" };
    const desktop: Record<string, unknown> = { id: "ws-1" };
    const added = fakeSignal();
    const removed = fakeSignal();
    const activated = fakeSignal();
    const screensChanged = fakeSignal();
    const desktopChanged = fakeSignal();
    const winFull = new Map<object, FakeSignal>();
    const winMax = new Map<object, FakeSignal>();
    const winDesktops = new Map<object, FakeSignal>();
    const winGeometry = new Map<object, FakeSignal>();
    const winInteractiveGeometry = new Map<object, FakeSignal>();
    const world: FakeWorld = {
        output,
        desktop,
        added,
        removed,
        wins: [],
        workspace: {},
        winFull,
        winMax,
        winDesktops,
        winGeometry,
        winInteractiveGeometry,
        maximizeClears: [],
    };
    const makeWin = (id: string, x: number): Record<string, unknown> => {
        const geo = fakeSignal();
        const full = fakeSignal();
        const max = fakeSignal();
        const desktopsChanged = fakeSignal();
        const interactive = fakeSignal();
        const win: Record<string, unknown> = {
            normalWindow: true,
            internalId: id,
            resourceClass: "test-app",
            output,
            desktops: [desktop],
            frameGeometry: { x, y: 0, width: 600, height: 800 },
            frameGeometryChanged: geo.signal,
            moveResizedChanged: interactive.signal,
            fullScreenChanged: full.signal,
            fullScreen: false,
            maximizedChanged: max.signal,
            maximizeMode: 0,
            desktopsChanged: desktopsChanged.signal,
            onAllDesktops: false,
            keepAbove: false,
            keepBelow: false,
        };
        win["setMaximize"] = (vertically: unknown, horizontally: unknown): void => {
            if (vertically !== false || horizontally !== false) {
                return;
            }
            world.maximizeClears.push(win);
            win["maximizeMode"] = 0;
            for (const handler of max.handlers) {
                handler();
            }
        };
        winFull.set(win, full);
        winMax.set(win, max);
        winDesktops.set(win, desktopsChanged);
        winGeometry.set(win, geo);
        winInteractiveGeometry.set(win, interactive);
        return win;
    };
    const winA = makeWin("win-a", 0);
    const winB = makeWin("win-b", 600);
    world.wins = [winA, winB];
    world.workspace["activeWindow"] = winA;
    world.workspace["windowList"] = (): unknown[] => [...world.wins];
    world.workspace["screens"] = [output];
    world.workspace["currentDesktopForScreen"] = (): unknown => desktop;
    world.workspace["clientArea"] = (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 });
    world.workspace["windowAdded"] = added.signal;
    world.workspace["windowRemoved"] = removed.signal;
    world.workspace["windowActivated"] = activated.signal;
    world.workspace["screensChanged"] = screensChanged.signal;
    world.workspace["currentDesktopChanged"] = desktopChanged.signal;
    return world;
}

interface EntryMocks {
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly shortcuts: Array<{ action: string; sequence: string; callback: () => void }>;
}

function startEntry(
    world: FakeWorld,
    overrides: PlanEntryOverrides = {},
): { handle: ReturnType<typeof startPlanAdapterEntry>; mocks: EntryMocks } {
    const mocks: EntryMocks = { dbusCalls: [], callbacks: [], timers: [], logs: [], shortcuts: [] };
    const handle = startPlanAdapterEntry({
        workspace: world.workspace,
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
        scheduleOnce: (delayMs, callback): (() => void) => {
            const entry = { delayMs, callback, cancelled: false };
            mocks.timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message): void => {
            mocks.logs.push(message);
        },
        owner: "owner-1",
        generation: "gen-1",
        registerShortcutFn: (action, _text, sequence, callback): boolean => {
            mocks.shortcuts.push({ action, sequence, callback });
            return true;
        },
        readProfileFn: (): string => "cosmic",
        ...overrides,
    });
    return { handle, mocks };
}

describe("plan entry live observation and shortcuts", () => {
    it("logs a bounded ready line identifying the plan session from existing provenance", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:ready owner=owner-1 generation=gen-1 source=local-dev"),
            "startup session-identifying context from the existing owner/generation provenance plus the local-dev source fallback",
        );
        handle?.stop();
    });

    it("registers the three MVP actions and observes stable ids", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(mocks.shortcuts.length, 129);
        const actions = mocks.shortcuts.map((row) => row.action);
        assert.equal(new Set(actions).size, 129);
        assert.ok(actions.includes("omnitiler-focus-left"));
        assert.ok(actions.includes("omnitiler-focus-right-arrow"));
        assert.ok(actions.includes("omnitiler-move-up"));
        assert.ok(actions.includes("omnitiler-resize-outwards-right"));
        assert.ok(actions.includes("omnitiler-resize-inwards-down"));
        assert.ok(actions.includes("omnitiler-resize-inwards-down-arrow"));
        assert.ok(actions.includes("omnitiler-toggle-float"));
        assert.ok(actions.includes("omnitiler-toggle-sticky"));
        assert.ok(actions.includes("omnitiler-toggle-maximize"));
        assert.ok(actions.includes("omnitiler-toggle-fullscreen"));
        assert.ok(actions.includes("omnitiler-toggle-workspace-tiling"));
        const toggle = mocks.shortcuts.find((row) => row.action === "omnitiler-toggle-workspace-tiling") as {
            sequence: string;
        };
        assert.equal(toggle.sequence, "Meta+Y");
        assert.ok(mocks.logs.includes("omnitiler:plan:shortcut-dispatch-shadowed action=omnitiler-toggle-float sequence=Meta+G holder_component=kwin holder_action=Grid View"));
        assert.ok(mocks.logs.includes("omnitiler:plan:shortcut-dispatch-shadowed action=omnitiler-toggle-maximize sequence=Meta+M holder_component=kwin holder_action=KrohnkiteMonocleLayout"));
        assert.ok(!mocks.logs.some((line) => line.includes("omnitiler-toggle-fullscreen") && line.includes("shadowed")), "Meta+F11 has no conflicting holder");
        const focus = mocks.shortcuts.find((row) => row.action === "omnitiler-focus-left") as {
            callback: () => void;
        };
        focus.callback();
        assert.equal(mocks.dbusCalls.length, 1);
        assert.equal(mocks.dbusCalls[0]?.method, "DescribePlan");
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(payload["command"], { op: "focus", window: "win-a", direction: "left" });
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            windows.map((entry) => entry["window"]),
            ["win-a", "win-b"],
        );
        handle?.stop();
    });

    it("unreadable foreground frame quarantines the whole domain, then resumes complete", () => {
        // Observation-convergence (2026-09-25): an unreadable foreground
        // member frame quarantines the whole foreground domain until a
        // complete observation is available, so a temporary frame read can
        // never trigger a transient remove/re-admit. Real production entry
        // (real foreground observeNative) on the signal-rich fake world.
        // Foreground is ws-1 with active win-a plus valid win-b.
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const focusLeft = mocks.shortcuts.find((row) => row.action === "omnitiler-focus-left") as {
            callback: () => void;
        };
        const windowsOf = (payload: string): string =>
            JSON.stringify((JSON.parse(payload) as Record<string, unknown>)["windows"] ?? payload);
        // Baseline: complete foreground observation dispatches both members.
        focusLeft.callback();
        assert.equal(mocks.dbusCalls.length, 1, "baseline foreground command dispatches");
        assert.ok(
            windowsOf(mocks.dbusCalls[0]?.payload as string).includes("win-a") &&
                windowsOf(mocks.dbusCalls[0]?.payload as string).includes("win-b"),
            "baseline carries the complete foreground membership",
        );
        const correlation = (JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>)[
            "correlation_id"
        ] as string;
        mocks.callbacks[0]?.(rejectedReply(correlation, "snapshot-invalid"));
        // Unreadable member frame with a valid active window: no partial
        // dispatch carrying only the survivors.
        const broken = world.wins[1];
        assert.ok(broken !== undefined, "second foreground member present");
        broken["frameGeometry"] = null;
        const before = mocks.dbusCalls.length;
        focusLeft.callback();
        const fresh = mocks.dbusCalls.slice(before);
        assert.ok(
            fresh.length === 0 ||
                fresh.every(
                    (call) =>
                        windowsOf(call.payload).includes("win-a") && windowsOf(call.payload).includes("win-b"),
                ),
            "quarantine: no partial-foreground dispatch on an unreadable member frame",
        );
        // Later complete observation resumes with full membership and no
        // transient removal of the survivor.
        broken["frameGeometry"] = { x: 600, y: 0, width: 600, height: 800 };
        const resumedBefore = mocks.dbusCalls.length;
        focusLeft.callback();
        const resumed = mocks.dbusCalls.slice(resumedBefore);
        const resumedComplete = resumed.filter(
            (call) => windowsOf(call.payload).includes("win-a") && windowsOf(call.payload).includes("win-b"),
        );
        assert.ok(resumedComplete.length > 0, "complete observation resumes dispatch with full membership");
        handle?.stop();
    });

    it("routes the internal float request through DescribePlan, writes the replied rectangle, and toggles back", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFloat();
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(payload["command"], { op: "toggle-float", window: "win-a" });
        const correlation = payload["correlation_id"] as string;
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1, correlation_id: correlation, outcome: "planned", desired_geometry: [{ window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        const active = world.wins[0] as Record<string, unknown>;
        assert.deepEqual(active["frameGeometry"], { x: 240, y: 160, width: 720, height: 480 });
        handle?.requestMove("left");
        assert.ok(mocks.logs.some((line) => line.includes("move-float-applied direction=left")));
        assert.deepEqual(active["frameGeometry"], { x: 8, y: 8, width: 588, height: 784 });
        handle?.requestFloat();
        const unfloat = JSON.parse(mocks.dbusCalls[1]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(unfloat["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 8, y: 8, w: 588, h: 784 } });
        mocks.callbacks[1]?.(JSON.stringify({
            v: 1, correlation_id: unfloat["correlation_id"], outcome: "planned", desired_geometry: [
                { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        assert.deepEqual(active["frameGeometry"], { x: 0, y: 0, width: 600, height: 800 });
        handle?.requestMove("left");
        assert.equal(mocks.dbusCalls[2]?.method, "DescribePlan");
        handle?.stop();
    });

    it("selects session-retained geometry and carries live float geometry across float/unfloat/float", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const active = world.wins[0] as Record<string, unknown>;
        // First float selects retained/centered geometry; the session replies
        // with the applied placement.
        handle?.requestFloat();
        const first = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(first["command"], { op: "toggle-float", window: "win-a" });
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1, correlation_id: first["correlation_id"], outcome: "planned", desired_geometry: [{ window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 240, y: 160, w: 720, h: 480 } },
        }));
        assert.deepEqual(active["frameGeometry"], { x: 240, y: 160, width: 720, height: 480 });
        // User moves the floating window.
        active["frameGeometry"] = { x: 300, y: 200, width: 500, height: 400 };
        // Unfloat carries the live (moved) geometry so the session retains it.
        handle?.requestFloat();
        const unfloat = JSON.parse(mocks.dbusCalls[1]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(unfloat["command"], { op: "toggle-float", window: "win-a", float_rect: { x: 300, y: 200, w: 500, h: 400 } });
        mocks.callbacks[1]?.(JSON.stringify({
            v: 1, correlation_id: unfloat["correlation_id"], outcome: "planned", desired_geometry: [
                { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                { window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
            ],
        }));
        // Re-float again selects retained geometry (no rect request), and the
        // session replies with the moved placement, not a recomputed center.
        handle?.requestFloat();
        const refloat = JSON.parse(mocks.dbusCalls[2]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(refloat["command"], { op: "toggle-float", window: "win-a" });
        mocks.callbacks[2]?.(JSON.stringify({
            v: 1, correlation_id: refloat["correlation_id"], outcome: "planned", desired_geometry: [{ window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } }],
            float_geometry: { window: "win-a", rect: { x: 300, y: 200, w: 500, h: 400 } },
        }));
        assert.deepEqual(active["frameGeometry"], { x: 300, y: 200, width: 500, height: 400 });
        handle?.stop();
    });

        it("refuses float on fullscreen and maximized targets without native writes", () => {        for (const [property, value, token] of [
            ["fullScreen", true, "omnitiler:plan:float-refused-fullscreen"],
            ["maximizeMode", 3, "omnitiler:plan:float-refused-maximize"],
        ] as const) {
            const world = fakeWorld();
            const active = world.wins[0] as Record<string, unknown>;
            active[property] = value;
            const before = active["frameGeometry"];
            const { handle, mocks } = startEntry(world);
            assert.ok(handle !== null);
            handle?.requestFloat();
            assert.equal(mocks.dbusCalls.length, 0);
            assert.equal(active["frameGeometry"], before);
            assert.ok(mocks.logs.includes(token));
            handle?.stop();
        }
    });

    it("toggles native fullscreen through the Meta+F11 callback without planner commands", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const row = mocks.shortcuts.find((entry) => entry.action === "omnitiler-toggle-fullscreen") as {
            sequence: string;
            callback: () => void;
        };
        assert.equal(row.sequence, "Meta+F11");
        const active = world.wins[0] as Record<string, unknown>;
        row.callback();
        assert.equal(active["fullScreen"], true);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-toggle window=win-a resource_class=test-app target=fullscreen outcome=invoked"));
        row.callback();
        assert.equal(active["fullScreen"], false);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.includes("omnitiler:plan:fullscreen-toggle window=win-a resource_class=test-app target=restored outcome=invoked"));
        handle?.stop();
    });

    it("issues one native maximize attempt per repeated toggle-maximize callback delivery", () => {
        // Shortcut-stream leg through the production entry seam: repeated
        // registered Meta+M callback deliveries each attempt once. Callbacks
        // are not physical proof: held-key KGlobalAccel repeat stays user-owned.
        const world = fakeWorld();
        const winA = world.wins[0] as Record<string, unknown>;
        const nativeValues: boolean[] = [];
        winA["setMaximize"] = (vertically: unknown, horizontally: unknown): void => {
            nativeValues.push(vertically === true && horizontally === true);
            winA["maximizeMode"] = vertically === false && horizontally === false ? 0 : 3;
            for (const handler of world.winMax.get(winA as object)?.handlers ?? []) {
                handler();
            }
        };
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const row = mocks.shortcuts.find((entry) => entry.action === "omnitiler-toggle-maximize") as {
            sequence: string;
            callback: () => void;
        };
        assert.equal(row.sequence, "Meta+M");
        row.callback();
        row.callback();
        row.callback();
        assert.deepEqual(nativeValues, [true, false, true], "each callback delivery attempts exactly once with alternating targets");
        assert.equal(winA["maximizeMode"], 3, "three deliveries end maximized");
        assert.equal(mocks.logs.filter((line) => line.includes("target=maximized outcome=issued")).length, 2, "two maximize targets issued");
        assert.equal(mocks.logs.filter((line) => line.includes("target=restored outcome=issued")).length, 1, "one restore target issued");
        assert.equal(mocks.dbusCalls.length, 0, "maximize toggle issues no planner commands");
        assert.ok(!mocks.logs.some((line) => line.includes("maximize-refused-attempted")), "per-activation semantics never refuse a repeated delivery");
        handle?.stop();
    });

    it("maps catalog rows to parameterized focus, move, and resize commands", () => {
        for (const profile of ["cosmic", "hyprland", "bspwm", "unknown"]) {
            const catalog = planShortcutCatalog(profile);
            assert.equal(catalog.length, 37);
            const byAction = new Map(catalog.map((row) => [row.action, row]));
            assert.equal(byAction.get("omnitiler-focus-up")?.direction, "up");
            assert.equal(byAction.get("omnitiler-focus-up")?.op, "focus");
            assert.equal(byAction.get("omnitiler-move-down-arrow")?.direction, "down");
            assert.equal(byAction.get("omnitiler-move-down-arrow")?.op, "move");
            assert.equal(byAction.get("omnitiler-resize-outwards-left")?.mode, "outwards");
            assert.equal(byAction.get("omnitiler-resize-outwards-left-arrow")?.mode, "outwards");
            assert.equal(byAction.get("omnitiler-resize-outwards-left-arrow")?.op, "resize");
            assert.equal(byAction.get("omnitiler-resize-outwards-left-arrow")?.direction, "left");
            assert.equal(byAction.get("omnitiler-resize-outwards-left-arrow")?.sequence, "Meta+Alt+Left");
            assert.equal(byAction.get("omnitiler-resize-inwards-left")?.mode, "inwards");
            assert.equal(byAction.get("omnitiler-resize-inwards-left-arrow")?.mode, "inwards");
            assert.equal(byAction.get("omnitiler-resize-inwards-left-arrow")?.op, "resize");
            assert.equal(byAction.get("omnitiler-resize-inwards-left-arrow")?.direction, "left");
            assert.deepEqual(byAction.get("omnitiler-toggle-float"), {
                action: "omnitiler-toggle-float", text: "Toggle floating window", sequence: "Meta+G", op: "float", direction: null, mode: null,
            });
            assert.equal(byAction.get("omnitiler-toggle-sticky")?.sequence, "Meta+Shift+G");
            assert.equal(byAction.get("omnitiler-toggle-maximize")?.sequence, "Meta+M");
            assert.deepEqual(byAction.get("omnitiler-toggle-fullscreen"), {
                action: "omnitiler-toggle-fullscreen", text: "Toggle fullscreen window", sequence: "Meta+F11", op: "fullscreen", direction: null, mode: null,
            });
            assert.deepEqual(byAction.get("omnitiler-toggle-orientation"), {
                action: "omnitiler-toggle-orientation", text: "Toggle split orientation", sequence: "Meta+O", op: "toggle-orientation", direction: null, mode: null,
            });
        }
    });

    it("adds grow and shrink arrow resize families", () => {
        for (const profile of ["cosmic", "hyprland", "bspwm", "unknown"]) {
            const catalog = planShortcutCatalog(profile);
            const byAction = new Map(catalog.map((row) => [row.action, row]));
            const expected: ReadonlyArray<{ direction: string; arrow: string }> = [
                { direction: "left", arrow: "Left" },
                { direction: "down", arrow: "Down" },
                { direction: "up", arrow: "Up" },
                { direction: "right", arrow: "Right" },
            ];
            for (const entry of expected) {
                const shrink = byAction.get(`omnitiler-resize-inwards-${entry.direction}-arrow`);
                assert.ok(shrink !== undefined, `${profile}:${entry.direction}:shrink`);
                assert.equal(shrink?.mode, "inwards");
                assert.equal(shrink?.op, "resize");
                assert.equal(shrink?.direction, entry.direction);
                assert.equal(shrink?.sequence, `Meta+Alt+Shift+${entry.arrow}`);
                const grow = byAction.get(`omnitiler-resize-outwards-${entry.direction}-arrow`);
                assert.ok(grow !== undefined, `${profile}:${entry.direction}:grow`);
                assert.equal(grow?.mode, "outwards");
                assert.equal(grow?.op, "resize");
                assert.equal(grow?.direction, entry.direction);
                assert.equal(grow?.sequence, `Meta+Alt+${entry.arrow}`);
            }
            const sequences = catalog.map((row) => row.sequence);
            assert.equal(new Set(sequences).size, catalog.length);
        }
    });

    it("routes each inwards resize arrow through the existing resize adapter", () => {
        for (const direction of ["left", "down", "up", "right"]) {
            const { handle, mocks } = startEntry(fakeWorld());
            assert.ok(handle !== null);
            const shortcut = mocks.shortcuts.find(
                (row) => row.action === `omnitiler-resize-inwards-${direction}-arrow`,
            ) as { callback: () => void };
            shortcut.callback();
            assert.equal(mocks.dbusCalls[0]?.method, "DescribePlan");
            const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(payload["command"], {
                op: "resize",
                window: "win-a",
                direction,
                mode: "inwards",
                press_index: 0,
            });
            handle?.stop();
        }
    });

    it("routes each outwards resize arrow through the existing resize adapter", () => {
        for (const direction of ["left", "down", "up", "right"]) {
            const { handle, mocks } = startEntry(fakeWorld());
            assert.ok(handle !== null);
            const shortcut = mocks.shortcuts.find(
                (row) => row.action === `omnitiler-resize-outwards-${direction}-arrow`,
            ) as { callback: () => void };
            shortcut.callback();
            assert.equal(mocks.dbusCalls[0]?.method, "DescribePlan");
            const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(payload["command"], {
                op: "resize",
                window: "win-a",
                direction,
                mode: "outwards",
                press_index: 0,
            });
            handle?.stop();
        }
    });

    it("routes the Meta+O toggle-orientation shortcut and handle facade through the orientation adapter", () => {
        const first = startEntry(fakeWorld());
        assert.ok(first.handle !== null);
        const shortcut = first.mocks.shortcuts.find(
            (row) => row.action === "omnitiler-toggle-orientation",
        ) as { sequence: string; callback: () => void };
        assert.equal(shortcut.sequence, "Meta+O");
        shortcut.callback();
        assert.equal(first.mocks.dbusCalls[0]?.method, "DescribePlan");
        const shortcutPayload = JSON.parse(first.mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(shortcutPayload["command"], { op: "toggle-orientation", window: "win-a" });
        first.handle?.stop();
        const second = startEntry(fakeWorld());
        assert.ok(second.handle !== null);
        second.handle?.requestToggleOrientation();
        assert.equal(second.mocks.dbusCalls[0]?.method, "DescribePlan");
        const facadePayload = JSON.parse(second.mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(facadePayload["command"], { op: "toggle-orientation", window: "win-a" });
        second.handle?.stop();
    });

    it("registers distinct Meta+Shift move sequences delivering op=move", () => {
        const first = startEntry(fakeWorld());
        assert.ok(first.handle !== null);
        const moves = first.mocks.shortcuts.filter(
            (row) => row.action.startsWith("omnitiler-move-") && !row.action.includes("workspace") && !row.action.includes("append"),
        );
        assert.equal(moves.length, 8);
        const sequences = moves.map((row) => row.sequence);
        assert.equal(new Set(sequences).size, 8);
        for (const row of moves) {
            assert.ok(row.sequence.startsWith("Meta+Shift+"), row.action);
        }
        const letter = moves.find((row) => row.action === "omnitiler-move-left") as {
            callback: () => void;
        };
        letter.callback();
        assert.equal(first.mocks.dbusCalls.length, 1);
        const letterPayload = JSON.parse(first.mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(letterPayload["command"], { op: "move", window: "win-a", direction: "left" });
        first.handle?.stop();
        const second = startEntry(fakeWorld());
        assert.ok(second.handle !== null);
        const arrow = second.mocks.shortcuts.find((row) => row.action === "omnitiler-move-right-arrow") as {
            callback: () => void;
        };
        arrow.callback();
        const arrowPayload = JSON.parse(second.mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(arrowPayload["command"], { op: "move", window: "win-a", direction: "right" });
        assert.equal(second.mocks.dbusCalls[0]?.method, "DescribePlan");
        second.handle?.stop();
    });

    it("registers production workspace number chords alongside the directional core", () => {
        for (const profile of ["cosmic", "hyprland", "bspwm", "unknown"]) {
            const catalog = planShortcutCatalog(profile);
            for (const row of catalog) {
                assert.ok(!row.action.includes("workspace"), `${profile}:${row.action}`);
                assert.ok(!/Meta(\+Shift)?\+\d/.test(row.sequence), `${profile}:${row.action}:${row.sequence}`);
                // F11 is the project fullscreen toggle, not a workspace digit chord.
                assert.ok(!/Meta\+\S*\d/.test(row.sequence) || row.sequence === "Meta+F11", `${profile}:${row.action}:${row.sequence}`);
            }
        }
        const live = startEntry(fakeWorld());
        assert.ok(live.handle !== null);
        const byAction = new Map(live.mocks.shortcuts.map((row) => [row.action, row]));
        assert.equal(live.mocks.shortcuts.length, 129);
        for (let index = 1; index <= 9; index += 1) {
            assert.equal(byAction.get(`omnitiler-workspace-${String(index)}`)?.sequence, `Meta+${String(index)}`);
            assert.equal(byAction.get(`omnitiler-move-workspace-${String(index)}`)?.sequence, `Meta+Shift+${String(index)}`);
        }
        assert.equal(byAction.get("omnitiler-workspace-0")?.sequence, "Meta+0");
        assert.equal(byAction.get("omnitiler-move-workspace-append")?.sequence, "Meta+Shift+0");
        const symbols: ReadonlyArray<[number, string]> = [[1, "!"], [2, "@"], [3, "#"], [4, "$"], [5, "%"], [6, "^"], [7, "&"], [8, "*"], [9, "("]];
        for (const [digit, symbol] of symbols) {
            assert.equal(byAction.get(`omnitiler-move-workspace-${String(digit)}-symbol`)?.sequence, `Meta+${symbol}`);
        }
        assert.equal(byAction.get("omnitiler-move-workspace-append-symbol")?.sequence, "Meta+)");
        for (const direction of ["left", "right", "up", "down"]) {
            assert.equal(byAction.get(`omnitiler-migrate-workspace-${direction}`)?.sequence, "");
        }
        live.handle?.stop();
        const entrySrc = readFileSync(join(kwinSrcDir(), "plan-adapter-entry.ts"), "utf8");
        assert.ok(entrySrc.includes("workspace-native"), "native lifecycle wiring");
        assert.ok(entrySrc.includes("WorkspaceSendAdapter"), "send transport reference");
        assert.ok(entrySrc.includes("workspaceShortcutCatalog"), "number chord catalog wiring");
    });

    it("observes only normal windows with normalized bare UUID ids", () => {
        const world = fakeWorld();
        world.wins.push({
            normalWindow: false,
            internalId: "dock-1",
            resourceClass: "org.kde.plasmashell",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 50, height: 50 },
            moveResizedChanged: fakeSignal().signal,
        });
        // An unreadable member frame quarantines the whole foreground domain
        // (covered by the dedicated quarantine row), so it cannot live in this
        // normal-only observation row.
        world.wins.push({
            normalWindow: true,
            internalId: "{12345678-1234-1234-1234-1234567890ab}",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 10, height: 10 },
            moveResizedChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
        });
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("right");
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        const ids = (payload["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"]);
        assert.deepEqual(ids, ["win-a", "win-b", "12345678-1234-1234-1234-1234567890ab"]);
        assert.ok(
            mocks.logs.some(
                (line) => line === "omnitiler:plan:observe-excluded reason=normal-window window=dock-1 resource_class=org.kde.plasmashell",
            ),
            "a non-normal window is excluded with its exact reason",
        );
        handle?.stop();
    });

    it("logs exact output and desktop exclusion reasons once per unchanged window", () => {
        const world = fakeWorld();
        const otherOutput = { name: "out-2" };
        const outputMismatch = {
            normalWindow: true,
            internalId: "other-output",
            resourceClass: "org.mozilla.firefox",
            output: otherOutput,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 10, height: 10 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
        };
        const desktopMismatch = {
            normalWindow: true,
            internalId: "other-desktop",
            resourceClass: "ghostty",
            output: world.output,
            desktops: [],
            frameGeometry: { x: 0, y: 0, width: 10, height: 10 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
        };
        world.wins.push(outputMismatch, desktopMismatch);
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("right");
        handle?.requestFocus("left");
        assert.equal(
            mocks.logs.filter((line) => line === "omnitiler:plan:observe-excluded reason=output-mismatch window=other-output resource_class=org.mozilla.firefox").length,
            1,
        );
        assert.equal(
            mocks.logs.filter((line) => line === "omnitiler:plan:observe-excluded reason=desktop-mismatch window=other-desktop resource_class=ghostty").length,
            1,
        );
        handle?.stop();
    });

    it("fails closed without logging on invalid scope or auth", () => {
        const world = fakeWorld();
        assert.equal(
            startEntry(world, { owner: "OWNER BANG", generation: "gen-1" }).handle,
            null,
        );
        assert.equal(startEntry(world, { owner: "owner-1", generation: "GEN BANG" }).handle, null);
        world.workspace["activeWindow"] = null;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "null-active startup keeps the enabled observer for the next window");
        assert.ok(
            mocks.logs.some((line) => line.includes("omnitiler:plan:ready")),
            "null-active startup still logs the truthful ready line",
        );
        handle?.stop();
    });

    it("uses only the DescribePlan transport for every command", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        handle?.requestMove("up");
        for (const call of mocks.dbusCalls) {
            assert.equal(call.method, "DescribePlan");
        }
        handle?.stop();
    });

    it("logs a bounded shortcut-failed line naming the exact chord and continues", () => {
        const world = fakeWorld();
        const attempts: string[] = [];
        const { handle, mocks } = startEntry(world, {
            registerShortcutFn: (action, _text, sequence, callback): boolean => {
                attempts.push(action);
                void callback;
                void sequence;
                if (action === "omnitiler-focus-left") {
                    return false;
                }
                return true;
            },
        });
        assert.ok(handle !== null);
        assert.equal(attempts.length, 129);
        assert.ok(attempts.includes("omnitiler-focus-right-arrow"));
        assert.ok(attempts.includes("omnitiler-resize-inwards-right-arrow"));
        const line = mocks.logs.find((entry) => entry.includes("shortcut-failed"));
        assert.ok(line !== undefined);
        assert.equal(
            line,
            "omnitiler:plan:shortcut-failed action=omnitiler-focus-left sequence=Meta+H",
        );
        handle?.stop();
    });

    it("keeps automatic tiling enabled when the shortcut catalog is missing or throws", () => {
        const key = "registerShortcut";
        const holder = globalThis as Record<string, unknown>;
        const had = Object.prototype.hasOwnProperty.call(holder, key);
        const saved = holder[key];
        const startWithoutShortcuts = (world: FakeWorld): { handle: ReturnType<typeof startPlanAdapterEntry>; mocks: EntryMocks } => {
            const mocks: EntryMocks = { dbusCalls: [], callbacks: [], timers: [], logs: [], shortcuts: [] };
            const handle = startPlanAdapterEntry({
                workspace: world.workspace,
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
                scheduleOnce: (delayMs, callback): (() => void) => {
                    const entry = { delayMs, callback, cancelled: false };
                    mocks.timers.push(entry);
                    return (): void => {
                        entry.cancelled = true;
                    };
                },
                log: (message): void => {
                    mocks.logs.push(message);
                },
                owner: "owner-1",
                generation: "gen-1",
                readProfileFn: (): string => "cosmic",
            });
            return { handle, mocks };
        };
        try {
            // Missing catalog: non-function lookup logs and keeps the handle.
            holder[key] = 0;
            const missingWorld = fakeWorld();
            const missing = startWithoutShortcuts(missingWorld);
            assert.ok(missing.handle !== null, "missing catalog keeps observation-driven tiling enabled");
            const missingLine = missing.mocks.logs.find((entry) =>
                entry.includes("omnitiler:plan:shortcut-catalog-unavailable"),
            );
            assert.ok(missingLine !== undefined, "missing catalog logs unavailable catalog");
            assert.ok(missingLine.includes("cause=register-shortcut-missing"));
            assert.ok(missingLine.includes("recovery=automatic-tiling-continue"));
            assert.ok(!missingLine.includes("window="), "catalog log carries no native ids");
            assert.ok(
                missing.mocks.logs.some((entry) => entry.includes("omnitiler:plan:ready")),
                "ready line still identifies the session",
            );
            missing.handle?.requestFocus("left");
            assert.equal(missing.mocks.dbusCalls[0]?.method, "DescribePlan");
            missing.handle?.stop();

            // Throwing lookup: bare global access throws, same bounded recovery.
            delete holder[key];
            const throwingWorld = fakeWorld();
            const throwing = startWithoutShortcuts(throwingWorld);
            assert.ok(throwing.handle !== null, "throwing lookup keeps observation-driven tiling enabled");
            const throwingLine = throwing.mocks.logs.find((entry) =>
                entry.includes("omnitiler:plan:shortcut-catalog-unavailable"),
            );
            assert.ok(throwingLine !== undefined, "throwing lookup logs unavailable catalog");
            assert.ok(
                throwingLine.includes("cause=register-shortcut-threw") ||
                    throwingLine.includes("cause=register-shortcut-missing"),
                `throwing lookup names a bounded cause, got ${throwingLine}`,
            );
            assert.ok(throwingLine.includes("recovery=automatic-tiling-continue"));
            throwing.handle?.requestFocus("left");
            assert.equal(throwing.mocks.dbusCalls[0]?.method, "DescribePlan");
            throwing.handle?.stop();
        } finally {
            if (had) {
                holder[key] = saved;
            } else {
                delete holder[key];
            }
        }
    });

    it("carries the bounded 8px domain gap in live DescribePlan requests", () => {
        assert.equal(DOMAIN_GAP, 8);
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["gap"], DOMAIN_GAP);
        handle?.stop();
    });

    it("carries the distinct 8px outer domain inset in live DescribePlan requests", () => {
        assert.equal(OUTER_DOMAIN_GAP, 8);
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["outer_gap"], OUTER_DOMAIN_GAP);
        handle?.stop();
    });

    it("keeps normalized ids stable across re-observation", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        handle?.requestFocus("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        const ids = (payload["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"]);
        assert.deepEqual(ids, ["win-a", "win-b"]);
        assert.deepEqual(payload["command"], { op: "focus", window: "win-a", direction: "left" });
        handle?.stop();
    });

    it("propagates a production Plan frameGeometry property rejection", () => {
        const world = fakeWorld();
        const rejected = world.wins[1] as Record<string, unknown>;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        handle?.requestMove("right");
        const payload = JSON.parse(mocks.dbusCalls[0]?.payload as string) as Record<string, unknown>;
        Object.defineProperty(rejected, "frameGeometry", {
            value: rejected["frameGeometry"],
            writable: false,
            configurable: true,
        });
        mocks.callbacks[0]?.(JSON.stringify({
            v: 1,
            correlation_id: payload["correlation_id"],
            outcome: "planned",
            desired_geometry: [
                { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 500, h: 800 } },
                { window: "win-b", leaf: "leaf-b", output: "out-1", workspace: "ws-1", rect: { x: 500, y: 0, w: 700, h: 800 } },
            ],
        }));
        assert.ok(mocks.logs.some((line) => line.includes("window=win-b") && line.includes("disposition=write-failed")), mocks.logs.join("\n"));
        assert.ok(mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=write-failed")), mocks.logs.join("\n"));
        assert.ok(!mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=planned-applied")), mocks.logs.join("\n"));
        handle?.stop();
    });

    it("subscribes shared Plan reflow to frameGeometryChanged, not the interactive signal", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as object;
        const { handle } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(world.winGeometry.get(winA)?.handlers.length, 1, "frameGeometryChanged is the production reflow source");
        assert.equal(world.winInteractiveGeometry.get(winA)?.handlers.length, 0, "interactive-only signal is never subscribed");
        const addedGeometry = fakeSignal();
        const addedInteractive = fakeSignal();
        const added = {
            normalWindow: true,
            internalId: "win-c",
            resourceClass: "test-app",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            frameGeometryChanged: addedGeometry.signal,
            moveResizedChanged: addedInteractive.signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
        };
        world.wins.push(added);
        for (const handler of world.added.handlers) {
            (handler as unknown as (value: unknown) => void)(added);
        }
        assert.equal(addedGeometry.handlers.length, 1, "added windows bind frame geometry changes");
        assert.equal(addedInteractive.handlers.length, 0, "added interactive signals stay unused");
        handle?.stop();
        assert.equal(world.winGeometry.get(winA)?.handlers.length, 0, "stop detaches frame geometry subscription");
        assert.equal(addedGeometry.handlers.length, 0, "stop detaches added-window frame geometry subscription");
    });

    it("subscribes fullScreenChanged per window after enable and for windows added later", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as object;
        const winB = world.wins[1] as object;
        const { handle } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(world.winFull.get(winA)?.handlers.length, 1, "existing window subscribed on enable");
        assert.equal(world.winFull.get(winB)?.handlers.length, 1, "existing window subscribed on enable");
        const winCFull = fakeSignal();
        const winCMax = fakeSignal();
        const winC = {
            normalWindow: true,
            internalId: "win-c",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: winCFull.signal,
            fullScreen: false,
            maximizedChanged: winCMax.signal,
            maximizeMode: 0,
        };
        world.wins.push(winC);
        world.winFull.set(winC, winCFull);
        world.winMax.set(winC, winCMax);
        const fireAdded = (win: object): void => {
            for (const handler of world.added.handlers) {
                (handler as (payload?: unknown) => void)(win);
            }
        };
        fireAdded(winC);
        assert.equal(winCFull.handlers.length, 1, "added window subscribed exactly once");
        fireAdded(winC);
        assert.equal(winCFull.handlers.length, 1, "no duplicate connection for the same window");
        handle?.stop();
        assert.equal(world.winFull.get(winA)?.handlers.length, 0, "stop detaches window subscriptions");
        assert.equal(winCFull.handlers.length, 0, "stop detaches added-window subscription");
    });

    it("subscribes maximizedChanged per window after enable and for windows added later", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as object;
        const winB = world.wins[1] as object;
        const { handle } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(world.winMax.get(winA)?.handlers.length, 1, "existing window subscribed on enable");
        assert.equal(world.winMax.get(winB)?.handlers.length, 1, "existing window subscribed on enable");
        const winCMax = fakeSignal();
        const winC = {
            normalWindow: true,
            internalId: "win-c",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: winCMax.signal,
            maximizeMode: 0,
        };
        world.wins.push(winC);
        world.winMax.set(winC, winCMax);
        const fireAdded = (win: object): void => {
            for (const handler of world.added.handlers) {
                (handler as (payload?: unknown) => void)(win);
            }
        };
        fireAdded(winC);
        assert.equal(winCMax.handlers.length, 1, "added window subscribed exactly once");
        fireAdded(winC);
        assert.equal(winCMax.handlers.length, 1, "no duplicate connection for the same window");
        handle?.stop();
        assert.equal(world.winMax.get(winA)?.handlers.length, 0, "stop detaches window subscriptions");
        assert.equal(winCMax.handlers.length, 0, "stop detaches added-window subscription");
    });

    it("collapses maximize modes 1 and 2 to the same unmaximize-first move as mode 3", () => {
        // G-06 (REQ-MAX-08): every nonzero maximize mode takes the same
        // unmaximize-first path (one native clear, then the ordinary move
        // once settlement is observed clear). The fake world settles
        // synchronously, so each mode clears once and dispatches.
        for (const mode of [1, 2, 3]) {
            const world = fakeWorld();
            (world.wins[0] as Record<string, unknown>)["maximizeMode"] = mode;
            const { handle, mocks } = startEntry(world);
            assert.ok(handle !== null);
            const move = mocks.shortcuts.find((row) => row.action === "omnitiler-move-left") as {
                callback: () => void;
            };
            move.callback();
            assert.equal(world.maximizeClears.length, 1, `mode ${String(mode)} clears once before moving`);
            assert.equal(mocks.dbusCalls.length, 1, `mode ${String(mode)} continues the ordinary move`);
            assert.ok(
                mocks.logs.some(
                    (line) => line.includes("move-maximize-clear") && line.includes("outcome=observed-cleared"),
                ),
                `mode ${String(mode)} moves only after observed clear`,
            );
            handle?.stop();
        }
        const world = fakeWorld();
        (world.wins[0] as Record<string, unknown>)["maximizeMode"] = 0;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const move = mocks.shortcuts.find((row) => row.action === "omnitiler-move-left") as {
            callback: () => void;
        };
        move.callback();
        assert.equal(mocks.dbusCalls.length, 1, "mode 0 dispatches normally");
        assert.deepEqual(world.maximizeClears, [], "mode 0 never clears");
        handle?.stop();
    });

    it("tiles startup when maximizedChanged cannot attach and logs once without identity", () => {
        const world = fakeWorld();
        for (const win of world.wins) {
            delete win["maximizedChanged"];
        }
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "missing maximize signal keeps tiling");
        const unavailable = mocks.logs.filter(
            (line) =>
                line ===
                "omnitiler:plan:maximize-signal-unavailable cause=maximizedChanged-unconnectable recovery=fresh-maximizeMode-read",
        );
        assert.equal(unavailable.length, 1, "single bounded unavailable line for all missing windows");
        assert.ok(
            !mocks.logs.some((line) => line === "omnitiler:plan:maximize-refused-signal"),
            "no global refusal token for a missing per-window signal",
        );
        assert.ok(
            !mocks.logs.some((line) => line.includes("win-a") || line.includes("win-b")),
            "unavailable line carries no window identity",
        );
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1, "missing-signal windows still dispatch");
        handle?.stop();
    });

    it("tiles startup when only one eligible window lacks maximizedChanged and keeps fresh maximize reads", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as object;
        const winB = world.wins[1] as object;
        delete (winA as Record<string, unknown>)["maximizedChanged"];
        (winA as Record<string, unknown>)["maximizeMode"] = 3;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "mixed availability keeps tiling");
        assert.equal(
            mocks.logs.filter(
                (line) =>
                    line ===
                    "omnitiler:plan:maximize-signal-unavailable cause=maximizedChanged-unconnectable recovery=fresh-maximizeMode-read",
            ).length,
            1,
            "single bounded unavailable line",
        );
        assert.equal(
            world.winMax.get(winB)?.handlers.length,
            1,
            "connectable window keeps its subscription",
        );
        const move = mocks.shortcuts.find((row) => row.action === "omnitiler-move-left") as {
            callback: () => void;
        };
        move.callback();
        // G-06 (REQ-MAX-08): the fresh-read maximized mover clears once
        // before moving; the fake world settles synchronously, so the
        // ordinary move follows in the same invocation.
        assert.equal(world.maximizeClears.length, 1, "fresh-read maximized mover clears once before moving");
        assert.equal(mocks.dbusCalls.length, 1, "observed-clear continues the ordinary move");
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("move-maximize-clear") && line.includes("outcome=observed-cleared"),
            ),
            "maximized move proceeds only after observed clear",
        );
        handle?.stop();
    });

    it("detaches maximizedChanged for a window removed after enable", () => {
        const world = fakeWorld();
        const { handle } = startEntry(world);
        assert.ok(handle !== null);
        const winCMax = fakeSignal();
        const winC = {
            normalWindow: true,
            internalId: "win-c",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: winCMax.signal,
            maximizeMode: 0,
        };
        world.wins.push(winC);
        world.winMax.set(winC, winCMax);
        for (const handler of world.added.handlers) {
            (handler as (payload?: unknown) => void)(winC);
        }
        assert.equal(winCMax.handlers.length, 1, "added window subscribed");
        for (const handler of world.removed.handlers) {
            (handler as (payload?: unknown) => void)(winC);
        }
        assert.equal(winCMax.handlers.length, 0, "removed window detached");
        handle?.stop();
    });

    it("keeps tiling when a window added after enable lacks maximizedChanged and logs once", () => {
        const world = fakeWorld();
        const winA = world.wins[0] as object;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(world.winMax.get(winA)?.handlers.length, 1, "existing window subscribed on enable");
        const winC = {
            normalWindow: true,
            internalId: "win-c",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
        };
        world.wins.push(winC);
        for (const handler of world.added.handlers) {
            (handler as (payload?: unknown) => void)(winC);
        }
        const winD = {
            normalWindow: true,
            internalId: "win-d",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 600, y: 0, width: 600, height: 800 },
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
        };
        world.wins.push(winD);
        for (const handler of world.added.handlers) {
            (handler as (payload?: unknown) => void)(winD);
        }
        assert.equal(
            mocks.logs.filter(
                (line) =>
                    line ===
                    "omnitiler:plan:maximize-signal-unavailable cause=maximizedChanged-unconnectable recovery=fresh-maximizeMode-read",
            ).length,
            1,
            "added-window misses share the single bounded line",
        );
        assert.ok(
            !mocks.logs.some((line) => line === "omnitiler:plan:maximize-refused-signal"),
            "no global refusal token for an added window without the signal",
        );
        assert.equal(
            world.winMax.get(winA)?.handlers.length,
            1,
            "prior maximize subscriptions are retained",
        );
        const callsBefore = mocks.dbusCalls.length;
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, callsBefore + 1, "enabled adapter still dispatches");
        assert.ok(
            !mocks.logs.some((line) => line === "omnitiler:plan:focus-refused-disabled"),
            "adapter stays enabled, never disabled by the missing signal",
        );
        handle?.stop();
    });

    it("refreshes the highlight exactly once after a completed move, never on focus or rejection", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world, {
            highlightCallDbus: (): void => {},
        });
        assert.ok(handle !== null);
        const activeGroupCalls = (): number =>
            mocks.dbusCalls.filter((call) => {
                try {
                    return (JSON.parse(call.payload) as Record<string, unknown>)["command"] !== undefined
                        && ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] === "active-group";
                } catch (error) {
                    void error;
                    return false;
                }
            }).length;
        assert.equal(activeGroupCalls(), 1, "highlight startup query");
        handle?.requestMove("right");
        const moveIndex = mocks.dbusCalls.length - 1;
        const movePayload = JSON.parse(mocks.dbusCalls[moveIndex]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(movePayload["command"], { op: "move", window: "win-a", direction: "right" });
        const moveCorr = movePayload["correlation_id"] as string;
        mocks.callbacks[moveIndex]?.(
            JSON.stringify({
                v: 1,
                correlation_id: moveCorr,
                outcome: "planned",
                desired_geometry: [
                    { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
            }),
        );
        assert.ok(mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=planned-applied")));
        assert.equal(activeGroupCalls(), 2, "exactly one observational refresh after completed move");
        handle?.requestFocus("left");
        const focusIndex = mocks.dbusCalls.length - 1;
        const focusPayload = JSON.parse(mocks.dbusCalls[focusIndex]?.payload as string) as Record<string, unknown>;
        const focusCorr = focusPayload["correlation_id"] as string;
        mocks.callbacks[focusIndex]?.(
            JSON.stringify({
                v: 1,
                correlation_id: focusCorr,
                outcome: "planned",
                desired_geometry: [
                    { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-b-leaf" },
            }),
        );
        assert.ok(mocks.logs.some((line) => line.includes("kind=focus") && line.includes("outcome=planned-applied")));
        assert.equal(activeGroupCalls(), 2, "focus-only plans never refresh via the geometry edge");
        handle?.requestMove("left");
        const rejIndex = mocks.dbusCalls.length - 1;
        const rejPayload = JSON.parse(mocks.dbusCalls[rejIndex]?.payload as string) as Record<string, unknown>;
        mocks.callbacks[rejIndex]?.(
            JSON.stringify({ v: 1, correlation_id: rejPayload["correlation_id"], outcome: "rejected", kind: "snapshot-invalid", message: "no" }),
        );
        assert.equal(activeGroupCalls(), 2, "rejected boundaries never refresh");
        handle?.stop();
    });

    it("retries a failed highlight attach on later applied/config events, logging once per transition", () => {
        const FAILED = "omnitiler:plan:highlight-attach stage=failed reason=bridge-unavailable";
        const RECOVERED = "omnitiler:plan:highlight-attach stage=recovered";
        const countActiveGroup = (mocks: EntryMocks): number =>
            mocks.dbusCalls.filter((call) => {
                try {
                    const command = (JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
                    return command["op"] === "active-group";
                } catch (error) {
                    void error;
                    return false;
                }
            }).length;
        // Flaky screensChanged: the adapter scope subscription (first
        // connect) succeeds so the entry starts, while the later highlight
        // domain subscription fails at startup and succeeds after recovery.
        const installFlakyScreens = (world: FakeWorld): { setFail: (fail: boolean) => void } => {
            const handlers: Array<() => void> = [];
            let connects = 0;
            let fail = true;
            world.workspace["screensChanged"] = {
                connect: (handler: () => void): void => {
                    connects += 1;
                    if (fail && connects > 1) throw new Error("screensChanged-unavailable");
                    handlers.push(handler);
                },
                disconnect: (handler: () => void): void => {
                    const index = handlers.indexOf(handler);
                    if (index >= 0) handlers.splice(index, 1);
                },
            };
            return { setFail: (next: boolean): void => { fail = next; } };
        };
        const fire = (signal: FakeSignal): void => {
            for (const handler of [...signal.handlers]) (handler as () => void)();
        };
        const succeedMove = (handle: ReturnType<typeof startPlanAdapterEntry>, mocks: EntryMocks): void => {
            handle?.requestMove("right");
            const moveIndex = mocks.dbusCalls.length - 1;
            const moveCorr = (JSON.parse(mocks.dbusCalls[moveIndex]?.payload as string) as Record<string, unknown>)["correlation_id"] as string;
            mocks.callbacks[moveIndex]?.(
                JSON.stringify({
                    v: 1,
                    correlation_id: moveCorr,
                    outcome: "planned",
                    desired_geometry: [
                        { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 600, h: 800 } },
                        { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 600, y: 0, w: 600, h: 800 } },
                    ],
                    desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
                }),
            );
            assert.ok(mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=planned-applied")));
        };
        // Applied-edge recovery.
        const worldA = fakeWorld();
        const flakyA = installFlakyScreens(worldA);
        const entryA = startEntry(worldA, { highlightCallDbus: (): void => {} });
        assert.ok(entryA.handle !== null, "entry starts while the highlight stays detached");
        assert.equal(countActiveGroup(entryA.mocks), 0, "no startup query after subscription failure");
        assert.equal(entryA.mocks.logs.filter((line) => line === FAILED).length, 1, "startup failure logged once");
        flakyA.setFail(false);
        succeedMove(entryA.handle, entryA.mocks);
        assert.equal(countActiveGroup(entryA.mocks), 1, "applied event reattaches the bridge");
        assert.equal(entryA.mocks.logs.filter((line) => line === RECOVERED).length, 1, "recovery logged once");
        succeedMove(entryA.handle, entryA.mocks);
        assert.equal(countActiveGroup(entryA.mocks), 2, "later applied only refreshes, never reattaches");
        assert.equal(entryA.mocks.logs.filter((line) => line === FAILED).length, 1, "no duplicate failure log");
        assert.equal(entryA.mocks.logs.filter((line) => line === RECOVERED).length, 1, "no duplicate recovery log");
        entryA.handle?.stop();
        // Config-edge recovery.
        const worldB = fakeWorld();
        const flakyB = installFlakyScreens(worldB);
        const configB = fakeSignal();
        const entryB = startEntry(worldB, {
            highlightCallDbus: (): void => {},
            options: { configChanged: configB.signal },
        });
        assert.ok(entryB.handle !== null);
        assert.equal(countActiveGroup(entryB.mocks), 0, "no startup query after subscription failure");
        assert.equal(entryB.mocks.logs.filter((line) => line === FAILED).length, 1, "startup failure logged once");
        fire(configB);
        assert.equal(entryB.mocks.logs.filter((line) => line === FAILED).length, 1, "still-failed retry stays quiet");
        assert.equal(countActiveGroup(entryB.mocks), 0, "still-failed retry sends nothing");
        flakyB.setFail(false);
        fire(configB);
        assert.equal(countActiveGroup(entryB.mocks), 1, "config event reattaches the bridge");
        assert.equal(entryB.mocks.logs.filter((line) => line === RECOVERED).length, 1, "recovery logged once");
        fire(configB);
        assert.equal(entryB.mocks.logs.filter((line) => line === RECOVERED).length, 1, "healthy config event stays quiet");
        assert.equal(countActiveGroup(entryB.mocks), 1, "healthy config event never reattaches");
        entryB.handle?.stop();
        // No retry after stop.
        const worldC = fakeWorld();
        const flakyC = installFlakyScreens(worldC);
        const configC = fakeSignal();
        const entryC = startEntry(worldC, {
            highlightCallDbus: (): void => {},
            options: { configChanged: configC.signal },
        });
        assert.ok(entryC.handle !== null);
        assert.equal(entryC.mocks.logs.filter((line) => line === FAILED).length, 1);
        entryC.handle?.stop();
        flakyC.setFail(false);
        fire(configC);
        assert.ok(!entryC.mocks.logs.some((line) => line === RECOVERED), "no recovery after stop");
        assert.equal(countActiveGroup(entryC.mocks), 0, "no attach after stop");
    });

    it("carries a fresh per-instance group stream across two entries while the Planner identity stays fixed", () => {
        const replyActiveGroup = (mocks: EntryMocks, effects: Array<{ method: string; payload: string | undefined }>): Record<string, unknown> => {
            const index = mocks.dbusCalls.findIndex((call) => {
                try {
                    return ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] === "active-group";
                } catch (error) {
                    void error;
                    return false;
                }
            });
            assert.ok(index >= 0, "startup active-group query");
            const request = JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
            assert.equal(request["owner"], "owner-1");
            assert.equal(request["generation"], "gen-1");
            const correlation = request["correlation_id"] as string;
            const callback = mocks.callbacks[index];
            assert.ok(callback !== undefined);
            callback(
                JSON.stringify({
                    v: 1,
                    correlation_id: correlation,
                    outcome: "active-group",
                    kind: "active-group",
                    base_revision: 2,
                    detail: {
                        kind: "active-group",
                        owner: "owner-1",
                        generation: "gen-1",
                        domain_output: "out-1",
                        domain_workspace: "ws-1",
                        group: "group-1",
                        focused_leaf: "leaf-win-a",
                        focused_window: "win-a",
                        members: [
                            { window: "win-a", leaf: "leaf-win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                            { window: "win-b", leaf: "leaf-win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                        ],
                        bounds: { x: 0, y: 0, w: 1200, h: 800 },
                    },
                }),
            );
            const setter = effects.find((entry) => entry.method === "SetGroupHighlight");
            assert.ok(setter?.payload !== undefined, "effect setter submitted");
            return JSON.parse(setter.payload as string) as Record<string, unknown>;
        };
        const collectEffects = (): { effects: Array<{ method: string; payload: string | undefined }>; override: PlanEntryOverrides } => {
            const effects: Array<{ method: string; payload: string | undefined }> = [];
            return {
                effects,
                override: {
                    highlightCallDbus: (_service, _path, _iface, method, ...args): void => {
                        effects.push({ method, payload: args[0] as string | undefined });
                    },
                },
            };
        };
        const first = collectEffects();
        const entryA = startEntry(fakeWorld(), { ...first.override, groupGeneration: "script-aaa1" });
        const second = collectEffects();
        const entryB = startEntry(fakeWorld(), { ...second.override, groupGeneration: "script-bbb2" });
        assert.ok(entryA.handle !== null && entryB.handle !== null);
        const effectA = replyActiveGroup(entryA.mocks, first.effects);
        const effectB = replyActiveGroup(entryB.mocks, second.effects);
        assert.equal(effectA["owner"], "owner-1");
        assert.equal(effectA["generation"], "script-aaa1");
        assert.equal(effectB["generation"], "script-bbb2");
        assert.notEqual(effectA["generation"], effectB["generation"]);
        entryA.handle?.stop();
        entryB.handle?.stop();
    });
});

describe("plan entry startup attach recovery", () => {
    const FAILED_PREFIX = "omnitiler:plan:entry-attach stage=failed";
    const RECOVERED = "omnitiler:plan:entry-attach stage=recovered";
    const fireAdded = (world: FakeWorld): void => {
        for (const handler of [...world.added.handlers]) (handler as () => void)();
    };

    it("recovers a transient startup lister throw on the next windowAdded without duplicating hooks", () => {
        const world = fakeWorld();
        let failLister = true;
        const wins = world.wins;
        world.workspace["windowList"] = (): unknown[] => {
            if (failLister) throw new Error("transient-list");
            return [...wins];
        };
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "transient enable failure waits instead of terminal null");
        const failed = mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX));
        assert.equal(failed.length, 1, "one bounded failed line");
        assert.ok(failed[0]?.includes("cause=geometry"), "failed line names the refused subscribe kind");
        assert.ok(!mocks.logs.some((line) => line.includes("omnitiler:plan:ready")), "no ready line while unavailable");
        assert.equal(mocks.shortcuts.length, 0, "no shortcut registration while unavailable");
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 0, "no actuation while unavailable");
        failLister = false;
        fireAdded(world);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "one bounded recovery line");
        assert.ok(mocks.logs.some((line) => line.includes("omnitiler:plan:ready")), "ready line after recovery");
        assert.equal(mocks.shortcuts.length, 129, "shortcuts register exactly once on recovery");
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1, "actuation resumes after recovery");
        assert.equal(mocks.dbusCalls[0]?.method, "DescribePlan");
        fireAdded(world);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "no duplicate recovery line");
        assert.equal(mocks.shortcuts.length, 129, "no duplicate shortcut registration");
        assert.equal(mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX)).length, 1, "no duplicate failed line");
        handle?.stop();
    });

    it("recovers on screensChanged alone after a refused startup without duplicating hooks", () => {
        const fireScreens = (screens: FakeSignal): void => {
            for (const handler of [...screens.handlers]) (handler as () => void)();
        };
        const world = fakeWorld();
        const screens = fakeSignal();
        world.workspace["screensChanged"] = screens.signal;
        let failLister = true;
        const wins = world.wins;
        world.workspace["windowList"] = (): unknown[] => {
            if (failLister) throw new Error("transient-list");
            return [...wins];
        };
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "transient enable failure waits instead of terminal null");
        assert.equal(mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX)).length, 1, "one bounded failed line");
        assert.equal(mocks.shortcuts.length, 0, "no shortcut registration while unavailable");
        failLister = false;
        fireScreens(screens);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "screensChanged alone recovers");
        assert.ok(mocks.logs.some((line) => line.includes("omnitiler:plan:ready")), "ready line after screens recovery");
        assert.equal(mocks.shortcuts.length, 129, "shortcuts register exactly once on screens recovery");
        fireScreens(screens);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "no duplicate recovery on later screen change");
        assert.equal(mocks.shortcuts.length, 129, "no duplicate shortcut registration on later screen change");
        assert.equal(mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX)).length, 1, "no duplicate failed line");
        handle?.stop();
        const pendingWorld = fakeWorld();
        const pendingScreens = fakeSignal();
        pendingWorld.workspace["screensChanged"] = pendingScreens.signal;
        pendingWorld.workspace["windowList"] = (): unknown[] => {
            throw new Error("persistent-list");
        };
        const pending = startEntry(pendingWorld);
        assert.ok(pending.handle !== null, "persistent failure still waits while pending");
        pending.handle?.stop();
        fireScreens(pendingScreens);
        assert.ok(!pending.mocks.logs.some((line) => line === RECOVERED), "no recovery after stop");
        assert.equal(pendingScreens.handlers.length, 0, "screens wait detaches on stop");
        assert.equal(pendingWorld.added.handlers.length, 0, "windowAdded wait detaches on stop");
    });

    it("stays terminal null when windowList is entirely missing", () => {
        const world = fakeWorld();
        delete (world.workspace as Record<string, unknown>)["windowList"];
        const { handle, mocks } = startEntry(world);
        assert.equal(handle, null, "genuinely missing lister stays terminal");
        assert.ok(!mocks.logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "terminal path stays silent");
    });

    it("stays terminal silent when native callDBus is missing and no override supplies it", () => {
        const world = fakeWorld();
        const logs: string[] = [];
        const handle = startPlanAdapterEntry({
            workspace: world.workspace,
            log: (message): void => {
                logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            scheduleOnce: (_delayMs, _callback): (() => void) => (): void => {},
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
        });
        assert.equal(handle, null, "missing transport stays terminal");
        assert.ok(!logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "missing transport never logs attach");
        for (const handler of [...world.added.handlers]) (handler as () => void)();
        assert.ok(!logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "missing transport never retries");
    });

    it("stays terminal silent when native QTimer is missing and no override supplies it", () => {
        const world = fakeWorld();
        const logs: string[] = [];
        const handle = startPlanAdapterEntry({
            workspace: world.workspace,
            callDbus: (_service, _path, _iface, _method, _payload, callback): void => {
                callback(true);
            },
            log: (message): void => {
                logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
        });
        assert.equal(handle, null, "missing timer stays terminal");
        assert.ok(!logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "missing timer never logs attach");
        for (const handler of [...world.added.handlers]) (handler as () => void)();
        assert.ok(!logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "missing timer never retries");
    });

    it("recovers a transient scope-signal failure and names the scope kind", () => {
        const world = fakeWorld();
        const saved = world.workspace["screensChanged"];
        delete (world.workspace as Record<string, unknown>)["screensChanged"];
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "transient scope failure waits instead of terminal null");
        const failed = mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX));
        assert.equal(failed.length, 1, "one bounded failed line");
        assert.ok(failed[0]?.includes("cause=scope"), "failed line names the scope kind");
        world.workspace["screensChanged"] = saved;
        fireAdded(world);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "one bounded recovery line");
        handle?.stop();
    });

    it("keeps invalid auth terminal null without retry tokens", () => {
        const world = fakeWorld();
        const bad = startEntry(world, { owner: "OWNER BANG", generation: "gen-1" });
        assert.equal(bad.handle, null);
        assert.ok(!bad.mocks.logs.some((line) => line.includes("omnitiler:plan:entry-attach")), "auth refusal stays silent");
    });

    it("recovers after more than five failed events with bounded transition logs", () => {
        const world = fakeWorld();
        let failLister = true;
        world.workspace["windowList"] = (): unknown[] => {
            if (failLister) throw new Error("persistent-list");
            return [...world.wins];
        };
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "persistent failure waits instead of terminal null");
        for (let index = 0; index < 10; index += 1) {
            fireAdded(world);
        }
        assert.ok(!mocks.logs.some((line) => line === RECOVERED), "no recovery while still failing");
        assert.equal(mocks.logs.filter((line) => line.startsWith(FAILED_PREFIX)).length, 1, "no repeated failed line");
        assert.ok(!mocks.logs.some((line) => line.includes("stage=exhausted")), "no exhaustion give-up");
        assert.equal(mocks.dbusCalls.length, 0, "no actuation while unavailable");
        failLister = false;
        fireAdded(world);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "one bounded recovery line");
        assert.ok(mocks.logs.some((line) => line.includes("omnitiler:plan:ready")), "ready line after recovery");
        assert.equal(mocks.shortcuts.length, 129, "shortcuts register exactly once on recovery");
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1, "actuation resumes after recovery");
        fireAdded(world);
        assert.equal(mocks.logs.filter((line) => line === RECOVERED).length, 1, "no duplicate recovery line");
        assert.equal(mocks.shortcuts.length, 129, "no duplicate shortcut registration");
        handle?.stop();
    });

    it("stop prevents any later retry", () => {
        const world = fakeWorld();
        let failLister = true;
        world.workspace["windowList"] = (): unknown[] => {
            if (failLister) throw new Error("persistent-list");
            return [...world.wins];
        };
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "persistent failure still waits while pending");
        handle?.stop();
        failLister = false;
        fireAdded(world);
        assert.ok(!mocks.logs.some((line) => line === RECOVERED), "no recovery after stop");
        assert.equal(mocks.dbusCalls.length, 0, "no actuation after stop");
        assert.ok(!mocks.logs.some((line) => line.includes("omnitiler:plan:ready")), "no ready line after stop");
    });
});

describe("plan adapter destroyed-window reply boundary", () => {
    it("drops a stale planned reply after a destroyed window without reading it and without writes", () => {
        const liveA: object = {};
        const liveC: object = {};
        let destroyedReads = 0;
        let revalidateCalls = 0;
        const destroyedB = new Proxy(
            {},
            {
                get(_target, _prop, _receiver): unknown {
                    destroyedReads += 1;
                    throw new Error("destroyed-window");
                },
            },
        );
        const refs = { a: liveA, b: destroyedB, c: liveC };
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                fingerprint: "fp-1",
                revalidate: (): boolean => {
                    revalidateCalls += 1;
                    return true;
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        const survivor = Object.freeze({
            id: "win-a",
            ref: refs.a,
            rect: { x: 0, y: 0, w: 100, h: 100 },
            output: "out-1",
            workspace: "ws-1",
            fullscreen: false,
            maximized: false,
        });
        mocks.observeImpl = (): PlanObserved | null => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-1",
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: 0,
            domainOuterGap: 0,
            focusedId: "win-a",
            windows: Object.freeze([survivor]),
            activeRef: refs.a,
            fingerprint: "fp-2",
            revalidate: (): boolean => {
                revalidateCalls += 1;
                return true;
            },
        });
        const writesBefore = mocks.geometries.length;
        assert.doesNotThrow(() => {
            mocks.callbacks[0]?.(
                plannedReply(
                    correlation,
                    [
                        { window: "win-a", rect: { x: 0, y: 0, w: 200, h: 200 } },
                        { window: "win-b", rect: { x: 200, y: 0, w: 100, h: 100 } },
                    ],
                    null,
                ),
            );
        });
        assert.equal(mocks.geometries.length, writesBefore);
        assert.equal(mocks.actives.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=stale-scope")));
        assert.equal(mocks.dbusCalls.length, 1, "membership change never replans; removal converges via observation");
        assert.equal(revalidateCalls, 0);
        assert.equal(destroyedReads, 0);
        assert.equal(adapter.isEnabled, true);
    });

    it("applies reconcile replies from a fresh observation covering survivors", () => {
        const refs = makeRefs();
        let ids: string[] = ["win-a", "win-b", "win-c"];
        const byId: Record<string, object> = { "win-a": refs.a, "win-b": refs.b, "win-c": refs.c };
        const mocks = mockEnv(refs);
        mocks.observeImpl = (): PlanObserved | null => {
            const wins = ids.map((id) =>
                Object.freeze({
                    id,
                    ref: byId[id] as object,
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    output: "out-1",
                    workspace: "ws-1",
                    fullscreen: false,
                    maximized: false,
                }),
            );
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-a",
                windows: Object.freeze(wins),
                activeRef: refs.a,
                fingerprint: `fp-${ids.join(",")}`,
                revalidate: () => true,
            };
        };
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.isEnabled, true);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        const admitCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        const writesAfterAdmit = mocks.geometries.length;
        assert.ok(writesAfterAdmit > 0);
        ids = ["win-a", "win-b"];
        fire(mocks, "removed");
        runTimers(mocks);
        const removeIndex = mocks.dbusCalls.length - 1;
        const removeCmd = plannerPayload(mocks, removeIndex)["command"] as Record<string, unknown>;
        assert.deepEqual(removeCmd, { op: "reconcile" });
        const removeCorr = plannerPayload(mocks, removeIndex)["correlation_id"] as string;
        const writesBeforeRemove = mocks.geometries.length;
        mocks.callbacks[removeIndex]?.(
            plannedReply(
                removeCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > writesBeforeRemove);
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
    });

    it("applies reconcile replies without reading the destroyed window while resolving live survivors", () => {
        const liveA: object = {};
        const liveB: object = {};
        let destroyedReads = 0;
        const destroyedC = new Proxy(
            {},
            {
                get(_target, _prop, _receiver): unknown {
                    destroyedReads += 1;
                    throw new Error("destroyed-window");
                },
            },
        );
        const byId: Record<string, object> = { "win-a": liveA, "win-b": liveB, "win-c": destroyedC };
        let ids: string[] = ["win-a", "win-b", "win-c"];
        const refs = { a: liveA, b: liveB, c: destroyedC };
        const mocks = mockEnv(refs);
        mocks.observeImpl = (): PlanObserved | null => {
            const wins = ids.map((id) =>
                Object.freeze({
                    id,
                    ref: byId[id] as object,
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    output: "out-1",
                    workspace: "ws-1",
                    fullscreen: false,
                    maximized: false,
                }),
            );
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: 0,
                domainOuterGap: 0,
                focusedId: "win-a",
                windows: Object.freeze(wins),
                activeRef: liveA,
                fingerprint: `fp-${ids.join(",")}`,
                revalidate: () => true,
            };
        };
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        const admitCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > 0);
        ids = ["win-a", "win-b"];
        fire(mocks, "removed");
        runTimers(mocks);
        const removeIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual(plannerPayload(mocks, removeIndex)["command"], { op: "reconcile" });
        const removeCorr = plannerPayload(mocks, removeIndex)["correlation_id"] as string;
        const writesBeforeRemove = mocks.geometries.length;
        const activesBefore = mocks.actives.length;
        assert.doesNotThrow(() => {
            mocks.callbacks[removeIndex]?.(
                plannedReply(
                    removeCorr,
                    [
                        { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                        { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                    ],
                    "win-a-leaf",
                ),
            );
        });
        assert.ok(mocks.geometries.length > writesBeforeRemove);
        for (let index = writesBeforeRemove; index < mocks.geometries.length; index += 1) {
            const target = mocks.geometries[index]?.target as object;
            assert.ok(target === liveA || target === liveB);
        }
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
        assert.equal(destroyedReads, 0);
        assert.equal(mocks.actives.length, activesBefore);
        assert.equal(adapter.isEnabled, true);
    });
});

describe("plan native identity sharing and string-keyed cache", () => {
    it("shares braced-UUID normalization with the plan entry with no local copy", () => {
        const dir = kwinSrcDir();
        const shared = readFileSync(join(dir, "native-id.ts"), "utf8");
        assert.ok(shared.includes("function normalizeNativeId"), "shared normalizer");
        assert.ok(shared.includes("unwrapBraced"), "braced handling");
        assert.ok(shared.includes("String("), "String(internalId)");
        const body = readFileSync(join(dir, "plan-adapter-entry.ts"), "utf8");
        assert.ok(body.includes("./native-id"), "plan entry uses shared utility");
        assert.ok(body.includes("normalizeNativeId"), "plan entry calls shared normalizer");
        const planEntry = readFileSync(join(dir, "plan-adapter-entry.ts"), "utf8");
        assert.ok(!planEntry.includes("function normalizeNativeId"), "plan entry uses shared, not a copy");
        assert.ok(!planEntry.includes("function unwrapBraced"), "plan entry uses shared, not a copy");
    });

    it("keeps plan entry identity string-keyed with explicit eviction and no WeakMap", () => {
        const dir = kwinSrcDir();
        const entry = readFileSync(join(dir, "plan-adapter-entry.ts"), "utf8");
        assert.ok(!entry.includes("WeakMap"), "no WeakMap");
        assert.ok(!entry.includes("OpaqueWindowIds"), "no OpaqueWindowIds");
        assert.ok(entry.includes("new Map<string, string>"), "Map<string, string> cache");
        assert.ok(entry.includes(".delete("), "explicit deletion");
        assert.ok(entry.includes("noteRemoved"), "eviction on adapter removed id");
        assert.ok(!entry.includes("windowRemoved\") as"), "no removal payload id read");
    });

    it("retains only primitive snapshots across the plan D-Bus boundary", () => {
        const dir = kwinSrcDir();
        const src = readFileSync(join(dir, "plan-adapter.ts"), "utf8");
        assert.ok(src.includes("snapshotOf"), "snapshot capture");
        assert.ok(src.includes("snapshotsEqual"), "snapshot comparison");
        assert.ok(src.includes("snapshot: PlanSnapshot"), "snapshot-typed flight/intent");
        assert.ok(src.includes("noteRemoved"), "removed-id eviction hook");
        assert.ok(!src.includes("flightState.observed"), "no retained observed");
        assert.ok(!src.includes("captured.revalidate"), "no retained revalidation call");
        assert.ok(!src.includes("observed: previous"), "no retained previous observed");
        assert.ok(!src.includes("observed: fresh"), "no retained fresh observed");
    });
});

describe("plan native id ownership on exact removal", () => {
    const exclusionCount = (mocks: EntryMocks, id: string): number =>
        mocks.logs.filter(
            (line) => line.includes("omnitiler:plan:observe-excluded") && line.includes(`window=${id}`),
        ).length;
    const windowsOf = (payload: string): string =>
        JSON.stringify((JSON.parse(payload) as Record<string, unknown>)["windows"] ?? payload);
    const focusLeft = (mocks: EntryMocks): (() => void) =>
        (mocks.shortcuts.find((row) => row.action === "omnitiler-focus-left") as { callback: () => void })
            .callback;

    it("evicts a pre-apply id on exact removal without reading the removed object", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        assert.equal(mocks.dbusCalls.length, 0, "startup interns without an applied plan");
        const winB = world.wins[1] as Record<string, unknown>;
        world.wins.splice(1, 1);
        Object.defineProperty(winB, "internalId", {
            configurable: true,
            get(): unknown {
                throw new Error("removed-object-unreadable");
            },
        });
        for (const handler of [...world.removed.handlers]) {
            (handler as (target?: unknown) => void)(winB);
        }
        // Re-admit a new live ref under a fresh id: the entry keeps working
        // and the exact removal never needed the removed object's properties.
        const winC = {
            normalWindow: true,
            internalId: "win-c",
            resourceClass: "test-app",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: { x: 600, y: 0, width: 600, height: 800 },
            frameGeometryChanged: fakeSignal().signal,
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
            desktopsChanged: fakeSignal().signal,
            onAllDesktops: false,
            keepAbove: false,
            keepBelow: false,
        };
        world.wins.push(winC);
        for (const handler of [...world.added.handlers]) {
            (handler as (target?: unknown) => void)(winC);
        }
        focusLeft(mocks)();
        assert.ok(mocks.dbusCalls.length > 0, "entry dispatches after exact removal");
        const last = mocks.dbusCalls[mocks.dbusCalls.length - 1]?.payload as string;
        assert.ok(windowsOf(last).includes("win-a") && windowsOf(last).includes("win-c"), "survivors carried");
        assert.ok(!windowsOf(last).includes("win-b"), "removed id not carried");
        handle?.stop();
    });

    it("never infers departure from an unreadable domain, then honors exact removal and stale reuse", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const winB = world.wins[1] as Record<string, unknown>;
        // Unreadable member frame quarantines the domain: no partial dispatch
        // carrying only the survivors, hence no inferred departure.
        winB["frameGeometry"] = null;
        const before = mocks.dbusCalls.length;
        focusLeft(mocks)();
        const fresh = mocks.dbusCalls.slice(before);
        assert.ok(
            fresh.length === 0 ||
                fresh.every((call) => windowsOf(call.payload).includes("win-a") && windowsOf(call.payload).includes("win-b")),
            "quarantine: incomplete observation never infers a departure",
        );
        assert.equal(exclusionCount(mocks, "win-b"), 1, "unreadable member reported once");
        // Exact removal while unreadable: evict by owner ref without reading
        // the removed object's properties.
        world.wins.splice(1, 1);
        Object.defineProperty(winB, "internalId", {
            configurable: true,
            get(): unknown {
                throw new Error("removed-object-unreadable");
            },
        });
        for (const handler of [...world.removed.handlers]) {
            (handler as (target?: unknown) => void)(winB);
        }
        // Reuse the id under a new live ref and make it unreadable again: the
        // eviction above cleared the exclusion dedup, so it reports once more.
        const winB2 = {
            normalWindow: true,
            internalId: "win-b",
            resourceClass: "test-app",
            output: world.output,
            desktops: [world.desktop],
            frameGeometry: null,
            frameGeometryChanged: fakeSignal().signal,
            moveResizedChanged: fakeSignal().signal,
            fullScreenChanged: fakeSignal().signal,
            fullScreen: false,
            maximizedChanged: fakeSignal().signal,
            maximizeMode: 0,
            desktopsChanged: fakeSignal().signal,
            onAllDesktops: false,
            keepAbove: false,
            keepBelow: false,
        };
        world.wins.push(winB2);
        for (const handler of [...world.added.handlers]) {
            (handler as (target?: unknown) => void)(winB2);
        }
        focusLeft(mocks)();
        assert.equal(exclusionCount(mocks, "win-b"), 2, "eviction cleared the removed id");
        // Stale removal under the old ref is a no-op: the reused id stays
        // owned by the new ref, so its exclusion dedup survives.
        for (const handler of [...world.removed.handlers]) {
            (handler as (target?: unknown) => void)(winB);
        }
        focusLeft(mocks)();
        assert.equal(exclusionCount(mocks, "win-b"), 2, "stale removal never evicts the new owner");
        handle?.stop();
    });
});

describe("plan adapter fullscreen isolation", () => {
    function makeObserved3(
        refs: { a: object; b: object; c: object },
        opts: {
            focused?: object;
            rects: Record<string, { x: number; y: number; w: number; h: number }>;
            fullscreen?: Record<string, boolean>;
            maximized?: Record<string, boolean>;
        },
        bounds: { x: number; y: number; w: number; h: number } = { x: 0, y: 0, w: 1200, h: 800 },
    ): PlanObserved {
        const focused = opts.focused ?? refs.a;
        const isFullscreen = (id: string): boolean => opts.fullscreen?.[id] === true;
        const isMaximized = (id: string): boolean => opts.maximized?.[id] === true;
        const refById: Record<string, object> = { "win-a": refs.a, "win-b": refs.b, "win-c": refs.c };
        const windows = Object.freeze(
            (["win-a", "win-b", "win-c"] as const)
                .filter((id) => opts.rects[id] !== undefined)
                .map((id) =>
                    Object.freeze({
                        id,
                        ref: refById[id] as object,
                        rect: opts.rects[id] as { x: number; y: number; w: number; h: number },
                        output: "out-1",
                        workspace: "ws-1",
                        fullscreen: isFullscreen(id),
                        maximized: isMaximized(id),
                    }),
                ),
        );
        return {
            domainOutput: "out-1",
            domainWorkspace: "ws-1",
            domainBounds: { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h },
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: focused === refs.a ? "win-a" : focused === refs.b ? "win-b" : "win-c",
            windows,
            activeRef: focused,
            fingerprint: "fp-fs",
            revalidate: () => true,
        };
    }

    function twoWindowBaseline(mocks: Mocks, refs: { a: object; b: object; c: object }): PlanAdapter {
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const corr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        return adapter;
    }

    it("subscribes the per-window fullScreenChanged signal kind", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        enableAdapter(mocks);
        assert.ok(mocks.subscribes.some((entry) => entry.kind === "fullscreen"));
    });

    it("refuses directional move/resize/pointer-resize on a fullscreen focused window with no dispatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        adapter.requestMove("left");
        adapter.requestResize("left", "outwards");
        assert.equal(adapter.requestPointerResize("win-b", "left", 600), false);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(mocks.geometries.length, 0);
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:move-refused-fullscreen"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:resize-refused-fullscreen"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:pointer-refused-fullscreen"));
    });

    it("holds a born-fullscreen member without a slot, then admits it on first exit", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        // win-c opens already fullscreen: it rides the wire as a synthetic
        // floating exception with no Rust tile slot while siblings fill space.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>), { op: "reconcile" });
        const sent = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            sent.find((entry) => entry["window"] === "win-c"),
            {
                window: "win-c",
                output: "out-1",
                workspace: "ws-1",
                rect: { x: 0, y: 0, w: 1200, h: 800 },
                floating: true,
                fullscreen: true,
                fit_excluded: true,
            },
            "born-fullscreen rides as a synthetic floating exception with no slot",
        );
        assert.ok(mocks.logs.includes("omnitiler:plan:initial-fullscreen-held window=win-c"));
        // Siblings fill the space alone: the reply covers only them.
        const corr = plannerPayload(mocks, 1)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${corr}`) && line.includes("outcome=planned-applied")));
        assert.ok(!mocks.geometries.slice(writesBefore).some((entry) => entry.target === refs.c), "held member never actuated");
        assert.ok(
            !mocks.logs.some((line) => line.startsWith("omnitiler:plan:write window=win-c")),
            "held member without a slot carries no write disposition",
        );
        // First exit admits at normal placement through one reconcile.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 100, h: 100 },
                },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 3);
        const admitIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, admitIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        assert.ok(mocks.logs.includes("omnitiler:plan:initial-fullscreen-released window=win-c"));
        const admitted = (plannerPayload(mocks, admitIndex)["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-c");
        assert.equal(admitted?.["floating"], undefined, "released member rides as a normal tile");
        const admitCorr = plannerPayload(mocks, admitIndex)["correlation_id"] as string;
        mocks.callbacks[admitIndex]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.ok(
            mocks.geometries.some((entry) => entry.target === refs.c && entry.rect.x === 800),
            "first exit admits the released window at its normal placement",
        );
        // Later fullscreen of that tiled window retains its slot: entering
        // fullscreen dispatches nothing and sibling drift skips its geometry.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        const callsBeforeFs = mocks.dbusCalls.length;
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBeforeFs, "tiled fullscreen keeps its slot without dispatch");
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 420, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBeforeFs + 1);
        const slotIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, slotIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const slotCorr = plannerPayload(mocks, slotIndex)["correlation_id"] as string;
        const slotWrites = mocks.geometries.length;
        mocks.callbacks[slotIndex]?.(
            plannedReply(
                slotCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(!mocks.geometries.slice(slotWrites).some((entry) => entry.target === refs.c), "retained slot never actuated");
        assert.ok(
            mocks.logs.some(
                (line) => line === "omnitiler:plan:write window=win-c resource_class=unknown disposition=skip-fullscreen rect=800,0,400,800",
            ),
            "later fullscreen retains its slot with a skip-fullscreen disposition",
        );
    });

    it("keeps a held born-fullscreen member floating across sibling drift without duplicate hold", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        const heldCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[1]?.(
            plannedReply(
                heldCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(!mocks.geometries.slice(writesBefore).some((entry) => entry.target === refs.c), "held member never actuated");

        // Repeated observation with sibling drift dispatches a fresh reconcile
        // while the held member stays a synthetic floating exception.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 616, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 3);
        const repeatIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, repeatIndex)["command"] as Record<string, unknown>), { op: "reconcile" });
        const repeatSent = plannerPayload(mocks, repeatIndex)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            repeatSent.find((entry) => entry["window"] === "win-c"),
            {
                window: "win-c",
                output: "out-1",
                workspace: "ws-1",
                rect: { x: 0, y: 0, w: 1200, h: 800 },
                floating: true,
                fullscreen: true,
                fit_excluded: true,
            },
            "held member stays a synthetic floating exception",
        );
        const repeatCorr = plannerPayload(mocks, repeatIndex)["correlation_id"] as string;
        const repeatWrites = mocks.geometries.length;
        mocks.callbacks[repeatIndex]?.(
            plannedReply(
                repeatCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${repeatCorr}`) && line.includes("outcome=planned-applied")));
        assert.ok(
            mocks.geometries.slice(repeatWrites).some((entry) => entry.target === refs.a && entry.rect.w === 600),
            "siblings retain full space",
        );
        assert.ok(!mocks.geometries.slice(repeatWrites).some((entry) => entry.target === refs.c), "held member never actuated");
        assert.ok(
            !mocks.logs.some((line) => line.startsWith("omnitiler:plan:write window=win-c")),
            "held member stays excluded from desired_geometry with no write disposition",
        );
        assert.equal(
            mocks.logs.filter((line) => line === "omnitiler:plan:initial-fullscreen-held window=win-c").length,
            1,
            "held exception logs exactly once",
        );
    });

    it("closes a held initial-fullscreen window without residue", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        const heldCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                heldCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.logs.includes("omnitiler:plan:initial-fullscreen-held window=win-c"));
        // Close while held: the departed member leaves no slot or hold behind.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const callsBefore = mocks.dbusCalls.length;
        fire(mocks, "removed", refs.c);
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore + 1);
        const removePayload = plannerPayload(mocks, callsBefore);
        assert.deepEqual((removePayload["command"] as Record<string, unknown>), { op: "reconcile" });
        assert.deepEqual(
            (removePayload["windows"] as Array<Record<string, unknown>>).map((entry) => entry["window"]),
            ["win-a", "win-b"],
            "the removal reconcile carries only survivors",
        );
        const removeCorr = removePayload["correlation_id"] as string;
        mocks.callbacks[callsBefore]?.(
            plannedReply(
                removeCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        // A later window reusing the domain tiles normally with no held residue.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 100, h: 100 },
                },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        const readmitIndex = mocks.dbusCalls.length - 1;
        assert.equal(mocks.dbusCalls.length, callsBefore + 2);
        const readmit = (plannerPayload(mocks, readmitIndex)["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-c");
        assert.equal(readmit?.["floating"], undefined, "no held residue pins the reused id as floating");
        const readmitCorr = plannerPayload(mocks, readmitIndex)["correlation_id"] as string;
        mocks.callbacks[readmitIndex]?.(
            plannedReply(
                readmitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.c), "the reused id tiles normally once re-added");
    });

    it("entering fullscreen from tiled adopts the baseline with no reconcile and no write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        const writesBefore = mocks.geometries.length;
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(mocks.geometries.length, writesBefore);
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(adapter.isEnabled, true);
    });

    it("leaving fullscreen restores the retained tiled position via one reconcile", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        // KWin restores the window imperfectly: reconcile must reassert the
        // retained slot rather than adopt the post-fullscreen position.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 100, y: 100, w: 400, h: 400 } },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        const reconcileIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, reconcileIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const corr = plannerPayload(mocks, reconcileIndex)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[reconcileIndex]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > writesBefore);
        assert.ok(
            mocks.geometries.some((entry) => entry.target === refs.b && entry.rect.x === 600 && entry.rect.y === 0),
            "retained position restored",
        );
        // The reconcile write restored the retained slot: adopt the converged
        // observation and confirm no further dispatch.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const callsAfter = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfter);
        assert.equal(adapter.isEnabled, true);
    });

    it("reflows siblings while one member is fullscreen without actuating it", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
            });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const admitCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
                fullscreen: { "win-a": true },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.b,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
                fullscreen: { "win-a": true },
            });
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2);
        const moveCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                moveCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 800, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 400, y: 0, w: 400, h: 800 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "sibling reflowed");
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.c), "sibling reflowed");
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "fullscreen member not actuated");
        assert.ok(mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=planned-applied")));
    });

    it("never targets a fullscreen member for reconcile and converges sibling drift in one attempt", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                fullscreen: { "win-a": true },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        const callsAfterEnter = mocks.dbusCalls.length;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 616, h: 800 } },
                fullscreen: { "win-a": true },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterEnter + 1);
        const reconcileIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, reconcileIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const corr = plannerPayload(mocks, reconcileIndex)["correlation_id"] as string;
        mocks.callbacks[reconcileIndex]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "reconcile never writes fullscreen member");
        // The write corrected the sibling: adopt the converged observation and
        // confirm one reconcile converged with no further dispatch and no park.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                fullscreen: { "win-a": true },
            });
        const callsAfter = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfter, "one reconcile converges; no park from fullscreen member");
        assert.equal(adapter.isEnabled, true);
    });

    it("reprojects a changed work area while retaining and not writing a fullscreen member", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        const writesBefore = mocks.geometries.length;
        const grown = { x: 200, y: 100, w: 800, h: 600 };
        const fullscreenObserved = (bounds: { x: number; y: number; w: number; h: number }): PlanObserved =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 0, w: 1600, h: 900 },
                        "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    },
                    fullscreen: { "win-a": true },
                },
                bounds,
            );
        mocks.observeImpl = () => fullscreenObserved({ x: 0, y: 0, w: 1200, h: 800 });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "entering fullscreen does not reproject");
        mocks.observeImpl = () => fullscreenObserved(grown);
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>), { op: "reconcile" });
        assert.deepEqual(plannerPayload(mocks, 1)["domain"], {
            output: "out-1",
            workspace: "ws-1",
            bounds: grown,
            gap: DOMAIN_GAP,
            outer_gap: OUTER_DOMAIN_GAP,
        });
        const carried = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        for (const entry of carried) {
            const rect = entry["rect"] as { x: number; y: number; w: number; h: number };
            assert.ok(rect.x >= grown.x && rect.y >= grown.y);
            assert.ok(rect.x + rect.w <= grown.x + grown.w);
            assert.ok(rect.y + rect.h <= grown.y + grown.h);
        }
        const correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 208, y: 108, w: 384, h: 584 } },
                    { window: "win-b", rect: { x: 608, y: 108, w: 384, h: 584 } },
                ],
                "win-a-leaf",
            ),
        );
        const applied = mocks.geometries.slice(writesBefore);
        assert.equal(applied.some((entry) => entry.target === refs.a), false);
        assert.deepEqual(applied, [{ target: refs.b, rect: { x: 608, y: 108, w: 384, h: 584 } }]);
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${correlation}`) && line.includes("outcome=planned-applied")));
    });

    it("routes simultaneous work-area and membership changes through reconcile", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        const grown = { x: 0, y: 0, w: 1600, h: 900 };
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 0, w: 600, h: 800 },
                        "win-b": { x: 600, y: 0, w: 600, h: 800 },
                        "win-c": { x: 0, y: 0, w: 100, h: 100 },
                    },
                },
                grown,
            );
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        const payload = plannerPayload(mocks, 1);
        assert.deepEqual(payload["command"], { op: "reconcile" });
        assert.deepEqual(payload["domain"], {
            output: "out-1",
            workspace: "ws-1",
            bounds: grown,
            gap: DOMAIN_GAP,
            outer_gap: OUTER_DOMAIN_GAP,
        });
        const correlation = payload["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[1]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 900 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 500, h: 900 } },
                    { window: "win-c", rect: { x: 1000, y: 0, w: 600, h: 900 } },
                ],
                "win-c-leaf",
            ),
        );
        const applied = mocks.geometries.slice(writesBefore);
        assert.deepEqual(
            new Set(applied.map((entry) => entry.target)),
            new Set([refs.a, refs.b, refs.c]),
        );
        assert.ok(applied.some((entry) => entry.target === refs.c && entry.rect.w === 600));
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${correlation}`) && line.includes("kind=reconcile") && line.includes("outcome=planned-applied")));
    });

    it("fullscreen-only rect drift adopts the baseline without dispatching reconcile", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 100, y: 50, w: 1000, h: 700 } },
                fullscreen: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(adapter.isEnabled, true);
    });

    it("carries a contained retained rect for an out-of-bounds fullscreen frame and reflows siblings", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // Work area excludes a 24px top panel; the fullscreen frame spans the
        // full physical area and exceeds these bounds.
        const bounds = { x: 0, y: 24, w: 1200, h: 776 };
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 24, w: 600, h: 776 },
                        "win-b": { x: 600, y: 24, w: 600, h: 776 },
                    },
                },
                bounds,
            );
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const admitCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 24, w: 600, h: 776 } },
                    { window: "win-b", rect: { x: 600, y: 24, w: 600, h: 776 } },
                ],
                "win-a-leaf",
            ),
        );
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                        "win-b": { x: 600, y: 24, w: 600, h: 776 },
                    },
                    fullscreen: { "win-a": true },
                },
                bounds,
            );
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        const callsAfterEnter = mocks.dbusCalls.length;
        // Sibling drift while the fullscreen frame is out of bounds must still
        // reconcile, and every carried window rect must stay in-bounds.
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                        "win-b": { x: 600, y: 24, w: 560, h: 776 },
                    },
                    fullscreen: { "win-a": true },
                },
                bounds,
            );
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterEnter + 1);
        const reconcileIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, reconcileIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const payload = plannerPayload(mocks, reconcileIndex);
        const db = (payload["domain"] as Record<string, unknown>)["bounds"] as { x: number; y: number; w: number; h: number };
        const carried = payload["windows"] as Array<Record<string, unknown>>;
        for (const entry of carried) {
            const rect = entry["rect"] as { x: number; y: number; w: number; h: number };
            assert.ok(
                rect.x >= db.x &&
                    rect.y >= db.y &&
                    rect.x + rect.w <= db.x + db.w &&
                    rect.y + rect.h <= db.y + db.h,
                `${String(entry["window"])} carried in-bounds`,
            );
        }
        const fullscreenCarried = carried.find((entry) => entry["window"] === "win-a") as Record<string, unknown>;
        assert.deepEqual(
            fullscreenCarried["rect"],
            { x: 0, y: 24, w: 600, h: 776 },
            "retained rect carried, never the compositor frame rect",
        );
        const corr = plannerPayload(mocks, reconcileIndex)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[reconcileIndex]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 24, w: 600, h: 776 } },
                    { window: "win-b", rect: { x: 600, y: 24, w: 600, h: 776 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > writesBefore, "sibling reflowed around the fullscreen member");
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "sibling written");
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "fullscreen member never actuated");
        assert.ok(mocks.logs.some((line) => line.includes("kind=reconcile") && line.includes("outcome=planned-applied")));
        assert.equal(adapter.isEnabled, true);
    });

    it("admits a window already fullscreen out-of-bounds with a contained carried rect", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const bounds = { x: 0, y: 24, w: 1200, h: 776 };
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                        "win-b": { x: 600, y: 24, w: 600, h: 776 },
                    },
                    fullscreen: { "win-a": true },
                },
                bounds,
            );
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        assert.equal(mocks.dbusCalls.length, 1);
        assert.deepEqual((plannerPayload(mocks, 0)["command"] as Record<string, unknown>)["op"], "reconcile");
        const payload = plannerPayload(mocks, 0);
        const db = (payload["domain"] as Record<string, unknown>)["bounds"] as { x: number; y: number; w: number; h: number };
        const carried = payload["windows"] as Array<Record<string, unknown>>;
        for (const entry of carried) {
            const rect = entry["rect"] as { x: number; y: number; w: number; h: number };
            assert.ok(
                rect.x >= db.x &&
                    rect.y >= db.y &&
                    rect.x + rect.w <= db.x + db.w &&
                    rect.y + rect.h <= db.y + db.h,
                `${String(entry["window"])} carried in-bounds`,
            );
        }
        const fullscreenCarried = carried.find((entry) => entry["window"] === "win-a") as Record<string, unknown>;
        assert.deepEqual(
            fullscreenCarried["rect"],
            { x: 0, y: 24, w: 1200, h: 776 },
            "clamped into bounds on admission, never the frame rect",
        );
        assert.equal(adapter.isEnabled, true);
    });
});

describe("plan adapter maximize isolation", () => {
    function makeObserved3(
        refs: { a: object; b: object; c: object },
        opts: {
            focused?: object;
            rects: Record<string, { x: number; y: number; w: number; h: number }>;
            fullscreen?: Record<string, boolean>;
            maximized?: Record<string, boolean>;
        },
        bounds: { x: number; y: number; w: number; h: number } = { x: 0, y: 0, w: 1200, h: 800 },
    ): PlanObserved {
        const focused = opts.focused ?? refs.a;
        const isFullscreen = (id: string): boolean => opts.fullscreen?.[id] === true;
        const isMaximized = (id: string): boolean => opts.maximized?.[id] === true;
        const refById: Record<string, object> = { "win-a": refs.a, "win-b": refs.b, "win-c": refs.c };
        const windows = Object.freeze(
            (["win-a", "win-b", "win-c"] as const)
                .filter((id) => opts.rects[id] !== undefined)
                .map((id) =>
                    Object.freeze({
                        id,
                        ref: refById[id] as object,
                        rect: opts.rects[id] as { x: number; y: number; w: number; h: number },
                        output: "out-1",
                        workspace: "ws-1",
                        fullscreen: isFullscreen(id),
                        maximized: isMaximized(id),
                    }),
                ),
        );
        return {
            domainOutput: "out-1",
            domainWorkspace: "ws-1",
            domainBounds: { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h },
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: focused === refs.a ? "win-a" : focused === refs.b ? "win-b" : "win-c",
            windows,
            activeRef: focused,
            fingerprint: "fp-max",
            revalidate: () => true,
        };
    }

    function twoWindowBaseline(mocks: Mocks, refs: { a: object; b: object; c: object }): PlanAdapter {
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const corr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        return adapter;
    }

    it("subscribes the per-window maximizedChanged signal kind", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        enableAdapter(mocks);
        assert.ok(mocks.subscribes.some((entry) => entry.kind === "maximize"));
    });

    class BornMaxEngineBridge {
        private readonly proc: ChildProcess;
        private readonly waiters: Array<{
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }> = [];
        private dead: string | null = null;

        private constructor(proc: ChildProcess) {
            this.proc = proc;
            const stdout = proc.stdout;
            if (stdout === null || stdout === undefined) {
                throw new Error("planner_eval stdout unavailable");
            }
            createInterface({ input: stdout }).on("line", (line: string) => {
                this.waiters.shift()?.resolve(line);
            });
            proc.stderr?.resume();
            proc.on("error", (error) => {
                this.failAll(error instanceof Error ? error : new Error(String(error)));
            });
            proc.on("exit", (code) => {
                this.failAll(new Error(`planner_eval exited with code ${String(code)}`));
            });
        }

        private failAll(error: Error): void {
            if (this.dead === null) {
                this.dead = error.message;
            }
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(error);
            }
        }

        private deathReason(): string | null {
            if (this.dead !== null) {
                return this.dead;
            }
            if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
                return "planner_eval process already exited";
            }
            return null;
        }

        static start(): BornMaxEngineBridge {
            return new BornMaxEngineBridge(
                spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                    cwd: bornMaxEngineRoot(),
                    stdio: ["pipe", "pipe", "pipe"],
                }),
            );
        }

        send(request: string): Promise<string> {
            const dead = this.deathReason();
            if (dead !== null) {
                return Promise.reject(new Error(dead));
            }
            return new Promise<string>((resolve, reject) => {
                this.waiters.push({ resolve, reject });
                try {
                    this.proc.stdin?.write(request + "\n");
                } catch (error) {
                    reject(error instanceof Error ? error : new Error(String(error)));
                }
            });
        }

        async close(): Promise<void> {
            if (this.deathReason() !== null) {
                try {
                    this.proc.stdin?.destroy();
                } catch (error) {
                    void error;
                }
                return;
            }
            try {
                this.proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolve) => {
                const timer = setTimeout(() => {
                    try {
                        this.proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolve();
                }, 3000);
                this.proc.on("exit", () => {
                    clearTimeout(timer);
                    resolve();
                });
            });
        }
    }

    function bornMaxEngineRoot(): string {
        let dir = resolve(process.cwd());
        for (let depth = 0; depth < 4; depth += 1) {
            try {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
            } catch (error) {
                void error;
            }
            dir = dirname(dir);
        }
        throw new Error("fixture root not found");
    }

    async function flushBornMaxPlan(mocks: Mocks, engine: BornMaxEngineBridge, index: number): Promise<Record<string, unknown>> {
        const call = mocks.dbusCalls[index];
        assert.ok(call !== undefined, `dispatch ${index} exists before flushing to the real Engine`);
        const replyText = await engine.send(call.payload);
        mocks.callbacks[index]?.(replyText);
        return JSON.parse(replyText) as Record<string, unknown>;
    }

    function bornMaxRect(reply: Record<string, unknown>, window: string): { x: number; y: number; w: number; h: number } {
        const geometry = reply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
        const entry = geometry?.find((candidate) => candidate["window"] === window);
        const rect = entry?.["rect"] as { x: number; y: number; w: number; h: number } | undefined;
        assert.ok(rect !== undefined, `Engine retains a slot for ${window}, got ${JSON.stringify(reply)}`);
        return rect;
    }

    it("born-maximized admission reserves a tile slot and restores into it", async () => {
        // R-MAX-06 keep-maximized-overlay: a window first seen maximized on
        // a tiled domain keeps native maximize (no clear or toggle). The
        // member reserves a tile slot as a maximized overlay (fit_excluded,
        // never actuated) while the sibling takes its allocated share; a
        // later native restore lands in the reserved slot, and repeated
        // maximize observations keep the slot with no further dispatch.
        // The tile-on-launch default clears instead (born-maximized.test.ts).
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.bornMode = "keep-maximized-overlay";
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                maximized: { "win-a": true },
                resourceClasses: { "win-a": "firefox" },
            });
        mocks.maximizeClearImpl = (): MaximizeClearOutcome => "invoked";
        const adapter = enableAdapter(mocks);
        const engine = BornMaxEngineBridge.start();
        try {
            adapter.requestResync();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 1, "admission dispatches one reconcile to reserve the slot");
            const admitPayload = plannerPayload(mocks, 0);
            assert.equal((admitPayload["command"] as Record<string, unknown>)["op"], "reconcile");
            const admitWindows = admitPayload["windows"] as Array<Record<string, unknown>>;
            assert.equal(admitWindows.find((entry) => entry["window"] === "win-a")?.["fit_excluded"], true, "maximized overlay rides fit_excluded");
            const admitReply = await flushBornMaxPlan(mocks, engine, 0);
            assert.equal(admitReply["outcome"], "planned", `admission plans, Engine replied ${JSON.stringify(admitReply)}`);
            const reserved = bornMaxRect(admitReply, "win-a");
            const share = bornMaxRect(admitReply, "win-b");
            assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "maximized overlay never actuated");
            assert.ok(
                mocks.geometries.some(
                    (entry) =>
                        entry.target === refs.b &&
                        entry.rect.x === share.x &&
                        entry.rect.y === share.y &&
                        entry.rect.w === share.w &&
                        entry.rect.h === share.h,
                ),
                "sibling takes its allocated share",
            );
            assert.ok(
                mocks.logs.some(
                    (line) =>
                        line ===
                        `omnitiler:plan:write window=win-a resource_class=firefox disposition=skip-maximized rect=${String(reserved.x)},${String(reserved.y)},${String(reserved.w)},${String(reserved.h)}`,
                ),
                "overlay carries skip-maximized with its reserved rect",
            );
            // Repeated admission observations while still maximized settle
            // with no clear, no writes, and no dispatch before native restore.
            // The sibling already reflects its allocated share after the
            // admission write; the overlay still reports its maximized frame
            // and carries the reserved slot.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": share },
                    maximized: { "win-a": true },
                    resourceClasses: { "win-a": "firefox" },
                });
            const callsAfterAdmit = mocks.dbusCalls.length;
            const writesAfterAdmit = mocks.geometries.length;
            fire(mocks, "maximize");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "repeated maximized admission stays quiet");
            assert.equal(mocks.geometries.length, writesAfterAdmit, "repeated maximized admission writes nothing");
            assert.equal(mocks.maximizeClears.length, 0, "repeated maximized admission clears nothing");
            assert.equal(mocks.maximizeToggles.length, 0, "repeated maximized admission toggles nothing");
            // Native restore lands in the reserved slot: KWin reports a wrong
            // plausible frame, the reconcile round-trips the real Engine, and
            // the applied write carries the reserved slot while the sibling
            // keeps its share.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 100, y: 100, w: 400, h: 400 }, "win-b": share },
                    resourceClasses: { "win-a": "firefox" },
                });
            fire(mocks, "maximize");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "restore dispatches one reconcile");
            const restoreReply = await flushBornMaxPlan(mocks, engine, 1);
            assert.equal(restoreReply["outcome"], "planned", `restore plans, Engine replied ${JSON.stringify(restoreReply)}`);
            assert.deepEqual(bornMaxRect(restoreReply, "win-a"), reserved, "retained slot survives the restore round-trip");
            assert.ok(
                mocks.geometries.some(
                    (entry) =>
                        entry.target === refs.a &&
                        entry.rect.x === reserved.x &&
                        entry.rect.y === reserved.y &&
                        entry.rect.w === reserved.w &&
                        entry.rect.h === reserved.h,
                ),
                "restore lands in the reserved slot",
            );
            for (const entry of mocks.geometries.filter((candidate) => candidate.target === refs.b)) {
                assert.deepEqual(entry.rect, share, "sibling keeps its share");
            }
            // Settled restore stays quiet.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": reserved, "win-b": share },
                    resourceClasses: { "win-a": "firefox" },
                });
            const callsAfterRestore = mocks.dbusCalls.length;
            const writesAfterRestore = mocks.geometries.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterRestore, "converged restore stays quiet");
            assert.equal(mocks.geometries.length, writesAfterRestore, "converged restore writes nothing");
            // Repeated maximize keeps the slot with no dispatch or writes.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": share },
                    maximized: { "win-a": true },
                    resourceClasses: { "win-a": "firefox" },
                });
            fire(mocks, "maximize");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterRestore, "repeated maximize keeps its slot without dispatch");
            assert.equal(mocks.geometries.length, writesAfterRestore, "repeated maximize writes nothing");
            assert.equal(mocks.maximizeClears.length, 0, "admission issues no native maximize clear");
            assert.equal(mocks.maximizeToggles.length, 0, "admission issues no native maximize toggle");
            assert.equal(adapter.isEnabled, true);
        } finally {
            await engine.close();
        }
    });

    it("born-maximized admission on a hidden domain reserves its slot", async () => {
        // R-MAX-06 keep-maximized-overlay: the reserved-slot overlay
        // contract holds on hidden tiled domains (no admission clear, a
        // fit_excluded dispatch the Engine retains, no actuation of the
        // overlay, and a quiet settle once converged).
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.bornMode = "keep-maximized-overlay";
        const hiddenRef = {};
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const hiddenSeen = (): PlanObserved => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-2",
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: "win-h",
            windows: Object.freeze([
                Object.freeze({
                    id: "win-h",
                    ref: hiddenRef,
                    rect: { x: 0, y: 0, w: 1200, h: 800 },
                    output: "out-1",
                    workspace: "ws-2",
                    fullscreen: false,
                    maximized: true,
                    resourceClass: "firefox",
                }),
            ]),
            activeRef: hiddenRef,
            fingerprint: "fp-hidden-max",
            revalidate: () => true,
        });
        (mocks.env as unknown as { observeHidden: () => ReadonlyArray<PlanObserved> }).observeHidden = () => [hiddenSeen()];
        mocks.maximizeClearImpl = (): MaximizeClearOutcome => "invoked";
        const adapter = enableAdapter(mocks);
        const engine = BornMaxEngineBridge.start();
        try {
            fire(mocks, "added");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 1, "foreground baseline dispatches");
            const baseReply = await flushBornMaxPlan(mocks, engine, 0);
            assert.equal(baseReply["outcome"], "planned", `baseline plans, Engine replied ${JSON.stringify(baseReply)}`);
            assert.equal(mocks.dbusCalls.length, 2, "hidden admission follows the foreground apply");
            const hiddenPayload = plannerPayload(mocks, 1);
            assert.equal((hiddenPayload["domain"] as Record<string, unknown>)["workspace"], "ws-2", "hidden admission targets its domain");
            const hiddenWindows = hiddenPayload["windows"] as Array<Record<string, unknown>>;
            assert.equal(hiddenWindows.find((entry) => entry["window"] === "win-h")?.["fit_excluded"], true, "hidden overlay rides fit_excluded");
            const hiddenReply = await flushBornMaxPlan(mocks, engine, 1);
            assert.equal(hiddenReply["outcome"], "planned", `hidden admission plans, Engine replied ${JSON.stringify(hiddenReply)}`);
            assert.ok(
                (hiddenReply["desired_geometry"] as Array<Record<string, unknown>>).some((entry) => entry["window"] === "win-h"),
                "Engine retains the hidden slot",
            );
            assert.ok(!mocks.geometries.some((entry) => entry.target === hiddenRef), "hidden overlay never actuated");
            // Settle: converged observations on both domains stay quiet.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": bornMaxRect(baseReply, "win-a"), "win-b": bornMaxRect(baseReply, "win-b") },
                });
            const callsAfter = mocks.dbusCalls.length;
            const writesAfter = mocks.geometries.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfter, "converged hidden domain stays quiet");
            assert.equal(mocks.geometries.length, writesAfter, "converged hidden domain writes nothing");
            assert.equal(mocks.maximizeClears.length, 0, "hidden admission issues no native maximize clear");
            assert.equal(adapter.isEnabled, true);
        } finally {
            await engine.close();
        }
    });

    it("workspace floating-to-tiled keeps maximize over a reserved slot (R-MAX-03)", async () => {
        // R-MAX-03 Q3: a first-seen maximized window on a floating workspace
        // keeps native maximize when the workspace toggles to tiled, under
        // both R-MAX-06 settings (the retile leg below runs the
        // tile-on-launch default). The retile issues no native clear or
        // toggle; the member reserves a tile slot as a maximized overlay
        // while the sibling takes its share. A later actual unmaximize
        // writes the retained slot, and a fresh born-maximized newcomer
        // reserves alongside through the overlay path (pinned to
        // keep-maximized-overlay: the default clears instead).
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = false;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = () => tiled;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                maximized: { "win-a": true },
                resourceClasses: { "win-a": "firefox" },
            });
        const adapter = enableAdapter(mocks);
        const engine = BornMaxEngineBridge.start();
        try {
            adapter.requestResync();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 0, "floating startup dispatches nothing");
            assert.equal(mocks.maximizeClears.length, 0, "floating startup clears nothing");
            assert.ok(mocks.logs.some((line) => line.includes("workspace-floating")), "floating workspace observed as floating");
            tiled = true;
            fire(mocks, "added");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 1, "retile dispatches one reconcile");
            assert.equal(mocks.maximizeClears.length, 0, "retile issues no native maximize clear");
            assert.equal(mocks.maximizeToggles.length, 0, "retile issues no native maximize toggle");
            const admitPayload = plannerPayload(mocks, 0);
            assert.deepEqual((admitPayload["command"] as Record<string, unknown>)["op"], "reconcile");
            const admitWindows = admitPayload["windows"] as Array<Record<string, unknown>>;
            assert.equal(admitWindows.find((entry) => entry["window"] === "win-a")?.["fit_excluded"], true, "maximized overlay rides fit_excluded");
            const admitReply = await flushBornMaxPlan(mocks, engine, 0);
            assert.equal(admitReply["outcome"], "planned", `retile plans, Engine replied ${JSON.stringify(admitReply)}`);
            const reserved = bornMaxRect(admitReply, "win-a");
            const share = bornMaxRect(admitReply, "win-b");
            assert.ok(reserved.w < 1200 && share.w < 1200, "both members receive a share, not a slotless maximum");
            assert.ok(
                reserved.x + reserved.w <= share.x || share.x + share.w <= reserved.x,
                "the sibling allocation leaves room for the reserved slot",
            );
            assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "maximized overlay never actuated");
            assert.ok(
                mocks.geometries.some(
                    (entry) =>
                        entry.target === refs.b &&
                        entry.rect.x === share.x &&
                        entry.rect.y === share.y &&
                        entry.rect.w === share.w &&
                        entry.rect.h === share.h,
                ),
                "sibling takes its allocated share",
            );
            assert.ok(
                mocks.logs.some(
                    (line) =>
                        line ===
                        `omnitiler:plan:write window=win-a resource_class=firefox disposition=skip-maximized rect=${String(reserved.x)},${String(reserved.y)},${String(reserved.w)},${String(reserved.h)}`,
                ),
                "overlay carries skip-maximized with its reserved rect",
            );
            // Repeated maximize signals while still maximized stay quiet.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": share },
                    maximized: { "win-a": true },
                    resourceClasses: { "win-a": "firefox" },
                });
            const callsAfterAdmit = mocks.dbusCalls.length;
            const writesAfterAdmit = mocks.geometries.length;
            fire(mocks, "maximize");
            fire(mocks, "maximize");
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "repeated maximized signals stay quiet");
            assert.equal(mocks.geometries.length, writesAfterAdmit, "repeated maximized signals write nothing");
            assert.equal(mocks.maximizeClears.length, 0, "repeated maximized signals clear nothing");
            assert.equal(mocks.maximizeToggles.length, 0, "repeated maximized signals toggle nothing");
            // Synchronous signals delivered during native geometry writes
            // settle quiet: arm the writer to echo sync signals, then report
            // an actual native unmaximize with a wrong plausible frame.
            const innerGeometry = mocks.geometryImpl;
            mocks.geometryImpl = (target, rect): boolean => {
                const ok = innerGeometry(target, rect);
                fire(mocks, "geometry", target);
                fire(mocks, "maximize", refs.a);
                return ok;
            };
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 100, y: 100, w: 400, h: 400 }, "win-b": share },
                    resourceClasses: { "win-a": "firefox" },
                });
            fire(mocks, "maximize");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "restore dispatches one reconcile");
            const restoreReply = await flushBornMaxPlan(mocks, engine, 1);
            assert.equal(restoreReply["outcome"], "planned", `restore plans, Engine replied ${JSON.stringify(restoreReply)}`);
            const restoreCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
            assert.ok(
                mocks.logs.some((line) => line.includes(`cmd=${restoreCorrelation}`) && line.includes("outcome=planned-applied")),
                "synchronous maximize/geometry write signals do not interrupt restore application",
            );
            assert.deepEqual(bornMaxRect(restoreReply, "win-a"), reserved, "retained slot survives the restore round-trip");
            assert.ok(
                mocks.geometries.some(
                    (entry) =>
                        entry.target === refs.a &&
                        entry.rect.x === reserved.x &&
                        entry.rect.y === reserved.y &&
                        entry.rect.w === reserved.w &&
                        entry.rect.h === reserved.h,
                ),
                "restore lands in the reserved slot",
            );
            for (const entry of mocks.geometries.filter((candidate) => candidate.target === refs.b)) {
                assert.deepEqual(entry.rect, share, "sibling keeps its share");
            }
            // Settled restore stays quiet despite the sync write echoes.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": reserved, "win-b": share },
                    resourceClasses: { "win-a": "firefox" },
                });
            const callsAfterRestore = mocks.dbusCalls.length;
            const writesAfterRestore = mocks.geometries.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterRestore, "converged restore stays quiet");
            assert.equal(mocks.geometries.length, writesAfterRestore, "converged restore writes nothing");
            assert.equal(mocks.maximizeClears.length, 0, "restore issues no native maximize clear");
            assert.equal(mocks.maximizeToggles.length, 0, "restore issues no native maximize toggle");
            // Born-max interaction: a fresh maximized newcomer on the same
            // tiled domain reserves through the overlay path under
            // keep-maximized-overlay (the tile-on-launch default clears
            // instead; born-maximized.test.ts).
            mocks.bornMode = "keep-maximized-overlay";
            mocks.observeImpl = () =>
                makeObserved3(refs, {
                    focused: refs.a,
                    rects: {
                        "win-a": reserved,
                        "win-b": share,
                        "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                    },
                    maximized: { "win-c": true },
                });
            fire(mocks, "added");
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterRestore + 1, "newcomer admission dispatches one reconcile");
            assert.equal(mocks.maximizeClears.length, 0, "newcomer admission issues no native maximize clear");
            const newcomerReply = await flushBornMaxPlan(mocks, engine, callsAfterRestore);
            assert.equal(newcomerReply["outcome"], "planned", `newcomer plans, Engine replied ${JSON.stringify(newcomerReply)}`);
            assert.ok(bornMaxRect(newcomerReply, "win-c") !== undefined, "Engine retains the newcomer slot");
            assert.ok(!mocks.geometries.some((entry) => entry.target === refs.c), "newcomer overlay never actuated");
            mocks.observeImpl = () =>
                makeObserved3(refs, {
                    focused: refs.a,
                    rects: {
                        "win-a": bornMaxRect(newcomerReply, "win-a"),
                        "win-b": bornMaxRect(newcomerReply, "win-b"),
                        "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                    },
                    maximized: { "win-c": true },
                });
            const callsAfterNewcomer = mocks.dbusCalls.length;
            const writesAfterNewcomer = mocks.geometries.length;
            fire(mocks, "maximize", refs.c);
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsAfterNewcomer, "born-maximized newcomer settles after R-MAX-03 restore");
            assert.equal(mocks.geometries.length, writesAfterNewcomer, "newcomer signals do not repeat geometry writes");
            assert.equal(mocks.maximizeClears.length, 0, "combined paths never clear maximize");
            assert.equal(mocks.maximizeToggles.length, 0, "combined paths never toggle maximize");
            assert.equal(adapter.isEnabled, true);
        } finally {
            await engine.close();
        }
    });

    it("normal floating-to-tiled admits without maximize handling", () => {
        // Control leg: a floating workspace with no maximized windows tiles
        // normally on toggle with geometry writes and no clear or toggle.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = false;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = () => tiled;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 0, "floating startup dispatches nothing");
        tiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "retile dispatches one reconcile");
        assert.equal(mocks.maximizeClears.length, 0, "normal retile clears nothing");
        assert.equal(mocks.maximizeToggles.length, 0, "normal retile toggles nothing");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.a), "retiled member takes its share");
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "sibling takes its share");
        assert.equal(adapter.isEnabled, true);
    });

    it("tiled admission never clears across later floating visits", () => {
        // R-MAX-03/R-MAX-06 share one overlay rule now: a maximized window
        // first seen on any domain keeps its maximize over a reserved slot.
        // G-06 (REQ-MAX-08) adds the directional move leg: a maximized move
        // attempts exactly one native clear first and, unconfirmed here,
        // moves nothing; later floating visits plus the first admission
        // retile issue no clear.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = true;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = () => tiled;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                maximized: { "win-a": true },
                resourceClasses: { "win-a": "firefox" },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("left");
        assert.equal(mocks.dbusCalls.length, 0, "unconfirmed clear moves nothing");
        assert.equal(mocks.maximizeClears.length, 1, "maximized move attempts exactly one native clear first");
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("move-refused-maximize-clear") && line.includes("cause=still-maximized"),
            ),
            "unconfirmed clear refuses narrowly with no structural move",
        );
        tiled = false;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 0, "floating visit dispatches nothing");
        assert.equal(mocks.maximizeClears.length, 1, "floating visit issues no further clear");
        tiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "first admission dispatches to reserve the slot");
        assert.equal(mocks.maximizeClears.length, 1, "tiled-first retile issues no further clear");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        const callsAfterAdmit = mocks.dbusCalls.length;
        tiled = false;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "post-apply floating visit dispatches nothing");
        tiled = true;
        fire(mocks, "maximize");
        runDebounce(mocks);
        assert.equal(mocks.maximizeClears.length, 1, "return to tile never clears a tiled-first origin");
        assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "return to tile stays quiet");
        assert.equal(adapter.isEnabled, true);
    });

    it("hidden floating-to-tiled keeps maximize over a reserved slot; reused id never clears", () => {
        // Hidden R-MAX-03 discriminator in two isolated windows: win-r3 proves
        // a hidden floating-to-tiled retile keeps maximize with zero clears;
        // win-h reuses the same id with a different exact ref first seen tiled
        // (no removed signal, so replacement not eviction) and must dispatch
        // its admission slot with zero clears. Neither window has an applied
        // slot or attempts before its admission, so any clear would be a
        // first-origin regression.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let hiddenTiled = false;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = (_output, workspace) =>
            workspace === "ws-2" ? hiddenTiled : true;
        const refR3 = {};
        const refOld = {};
        const refNew = {};
        let hiddenId = "win-r3";
        let hiddenRefCurrent: object = refR3;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const hiddenSeen = (): PlanObserved => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-2",
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: hiddenId,
            windows: Object.freeze([
                Object.freeze({
                    id: hiddenId,
                    ref: hiddenRefCurrent,
                    rect: { x: 0, y: 0, w: 1200, h: 800 },
                    output: "out-1",
                    workspace: "ws-2",
                    fullscreen: false,
                    maximized: true,
                    resourceClass: "firefox",
                }),
            ]),
            activeRef: hiddenRefCurrent,
            fingerprint: `fp-${hiddenId}`,
            revalidate: () => true,
        });
        (mocks.env as unknown as { observeHidden: () => ReadonlyArray<PlanObserved> }).observeHidden = () => [hiddenSeen()];
        mocks.maximizeClearImpl = (target): MaximizeClearOutcome => {
            assert.ok(target === hiddenRefCurrent, "clear targets the exact current ref");
            return "invoked";
        };
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "foreground baseline dispatches");
        const baseCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                baseCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        // Stage 1: hidden R-MAX-03 proof with a dedicated window.
        const callsAfterBase = mocks.dbusCalls.length;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterBase, "hidden floating startup dispatches nothing");
        assert.equal(mocks.maximizeClears.length, 0, "hidden floating startup clears nothing");
        hiddenTiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterBase + 1, "hidden retile dispatches one reconcile");
        assert.equal(mocks.maximizeClears.length, 0, "hidden retile issues no native maximize clear");
        assert.equal(mocks.maximizeToggles.length, 0, "hidden retile issues no native maximize toggle");
        const hiddenCorr = plannerPayload(mocks, callsAfterBase)["correlation_id"] as string;
        const hiddenWindows = plannerPayload(mocks, callsAfterBase)["windows"] as Array<Record<string, unknown>>;
        assert.equal(hiddenWindows.find((entry) => entry["window"] === "win-r3")?.["fit_excluded"], true, "hidden overlay rides fit_excluded");
        mocks.callbacks[callsAfterBase]?.(
            plannedReply(hiddenCorr, [{ window: "win-r3", rect: { x: 0, y: 0, w: 1200, h: 800 } }], null),
        );
        assert.ok(
            mocks.logs.some((line) => line.includes(`cmd=${hiddenCorr}`) && line.includes("outcome=planned-applied")),
            "hidden retile applies its slot",
        );
        // Stage 2: observe a fresh id on the floating domain WITHOUT retile,
        // then reuse the id with a new ref first seen tiled. No removed
        // signal, no apply, no attempts for win-h before replacement.
        hiddenId = "win-h";
        hiddenRefCurrent = refOld;
        hiddenTiled = false;
        fire(mocks, "added");
        runDebounce(mocks);
        const callsBeforeReuse = mocks.dbusCalls.length;
        assert.equal(mocks.dbusCalls.length, callsBeforeReuse, "reuse-id floating capture dispatches nothing");
        assert.equal(mocks.maximizeClears.length, 0, "reuse-id floating capture clears nothing");
        hiddenRefCurrent = refNew;
        hiddenTiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBeforeReuse + 1, "reused id dispatches its admission");
        assert.equal(mocks.maximizeClears.length, 0, "reused id with a new ref issues no clear");
        const reuseIndex = callsBeforeReuse;
        const reusePayload = plannerPayload(mocks, reuseIndex);
        assert.equal((reusePayload["domain"] as Record<string, unknown>)["workspace"], "ws-2", "reuse admission targets its domain");
        assert.equal(
            (reusePayload["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-h")?.["fit_excluded"],
            true,
            "reused overlay rides fit_excluded",
        );
        const reuseCorr = reusePayload["correlation_id"] as string;
        mocks.callbacks[reuseIndex]?.(
            plannedReply(reuseCorr, [{ window: "win-h", rect: { x: 0, y: 0, w: 1200, h: 800 } }], null),
        );
        assert.ok(
            mocks.logs.some((line) => line.includes(`cmd=${reuseCorr}`) && line.includes("outcome=planned-applied")),
            "reused admission applies its slot",
        );
        assert.ok(!mocks.geometries.some((entry) => entry.target === refNew), "reused overlay never actuated");
        assert.equal(adapter.isEnabled, true);
    });

    it("leaves fullscreen admission isolated even when maximize is also set", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // A window born fullscreen and maximized is held without a slot:
        // fullscreen takes precedence over the maximize admission clear and
        // the member rides as a synthetic floating exception.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                fullscreen: { "win-a": true },
                maximized: { "win-a": true },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.maximizeClears.length, 0, "fullscreen takes precedence over admission clear");
        const sent = plannerPayload(mocks, 0)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            sent.find((entry) => entry["window"] === "win-a"),
            {
                window: "win-a",
                output: "out-1",
                workspace: "ws-1",
                rect: { x: 0, y: 0, w: 1200, h: 800 },
                floating: true,
                fullscreen: true,
                fit_excluded: true,
            },
            "born fullscreen-maximized rides held as a synthetic floating exception",
        );
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(correlation, [{ window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } }], "win-b-leaf"),
        );
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${correlation}`) && line.includes("outcome=planned-applied")));
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "held member never actuated");
        assert.ok(
            !mocks.logs.some((line) => line.startsWith("omnitiler:plan:write window=win-a")),
            "held member without a slot carries no write disposition",
        );
    });

    it("clears maximize once on first exit of a held born-fullscreen maximized window", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        let fullscreenC = true;
        let maximizedC = true;
        mocks.maximizeClearImpl = (target): MaximizeClearOutcome => {
            assert.equal(target, refs.c);
            maximizedC = false;
            fire(mocks, "maximize", refs.c);
            return "invoked";
        };
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: fullscreenC ? { "win-c": true } : {},
                maximized: maximizedC ? { "win-c": true } : {},
            });
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.maximizeClears.length, 0, "no clear while fullscreen");
        assert.ok(mocks.logs.includes("omnitiler:plan:initial-fullscreen-held window=win-c"));
        const heldCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                heldCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${heldCorr}`) && line.includes("outcome=planned-applied")));
        // First non-fullscreen observation is still maximized: one native
        // clear runs before the normal fresh admission, even though the
        // prior synthetic floating exception already left applied evidence.
        fullscreenC = false;
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.deepEqual(mocks.maximizeClears, [refs.c], "exactly one admission clear on first exit");
        assert.ok(mocks.logs.includes("omnitiler:plan:initial-fullscreen-released window=win-c"));
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:maximize-admission-clear window=win-c resource_class=unknown outcome=observed-cleared"),
            "clear observed before fresh admission",
        );
        assert.equal(mocks.dbusCalls.length, 3);
        const admitIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, admitIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const admitted = (plannerPayload(mocks, admitIndex)["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-c");
        assert.equal(admitted?.["floating"], undefined, "released member rides as a normal tile");
        assert.equal(admitted?.["fit_excluded"], undefined, "cleared member rejoins the fit");
        const admitCorr = plannerPayload(mocks, admitIndex)["correlation_id"] as string;
        mocks.callbacks[admitIndex]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.ok(
            mocks.geometries.some((entry) => entry.target === refs.c && entry.rect.x === 800),
            "first exit admits the released window at its normal placement",
        );
        // One-shot: the converged observation stays quiet with no retry.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
            });
        const callsAfterAdmit = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "converged admission stays quiet");
        assert.deepEqual(mocks.maximizeClears, [refs.c], "no retry after the one-shot clear");
        // A later re-maximize of the admitted window never re-clears.
        maximizedC = true;
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                maximized: { "win-c": true },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        assert.deepEqual(mocks.maximizeClears, [refs.c], "post-admission maximize never re-clears");
        assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "post-admission maximize keeps its slot without dispatch");
        // An already tiled window taking fullscreen plus maximize keeps its
        // slot with no admission clear.
        fullscreenC = true;
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
                maximized: { "win-c": true },
            });
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.deepEqual(mocks.maximizeClears, [refs.c], "already tiled fullscreen-maximized never clears");
        assert.equal(mocks.dbusCalls.length, callsAfterAdmit, "already tiled overlay keeps its slot without dispatch");
    });

    it("unmaximize-first directional move on a maximized focused window; resize/pointer-resize still refuse", () => {
        // G-06 (REQ-MAX-08): directional move of a maximized window
        // unmaximizes first (one native attempt; unconfirmed here, so no
        // structural move), while resize and pointer-resize keep their
        // maximized refusals with no dispatch.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                maximized: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        adapter.requestMove("left");
        adapter.requestResize("left", "outwards");
        assert.equal(adapter.requestPointerResize("win-b", "left", 600), false);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.maximizeClears.length, 1, "maximized move attempts exactly one native clear first");
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("move-refused-maximize-clear") && line.includes("cause=still-maximized"),
            ),
            "unconfirmed clear refuses narrowly with no structural move",
        );
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:move-refused-maximize"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:resize-refused-maximize"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:pointer-refused-maximize"));
    });

    it("prefers fullscreen over maximize for action refusal tokens", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                fullscreen: { "win-b": true },
                maximized: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        adapter.requestMove("left");
        adapter.requestResize("left", "outwards");
        assert.equal(adapter.requestPointerResize("win-b", "left", 600), false);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:move-refused-fullscreen"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:resize-refused-fullscreen"));
        assert.ok(mocks.logs.some((line) => line === "omnitiler:plan:pointer-refused-fullscreen"));
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:move-refused-maximize"));
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:resize-refused-maximize"));
        assert.ok(!mocks.logs.some((line) => line === "omnitiler:plan:pointer-refused-maximize"));
    });

    it("admits a maximized member into the tree but never writes its geometry", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                maximized: { "win-c": true },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2);
        const cmd = plannerPayload(mocks, 1)["command"] as Record<string, unknown>;
        assert.deepEqual(cmd, { op: "reconcile" });
        const sent = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        assert.ok(sent.some((entry) => entry["window"] === "win-c"), "maximized member stays observed");
        const corr = plannerPayload(mocks, 1)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > writesBefore, "siblings reflowed around the admission");
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.c), "maximized member never actuated");
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:write window=win-c resource_class=unknown disposition=skip-maximized rect=800,0,400,800",
            ),
            "maximized member carries skip-maximized disposition with its retained target rect",
        );
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
    });

    it("carries a known member's retained rect when KWin transiently reports its frame out of bounds", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const bounds = { x: 0, y: 24, w: 1200, h: 776 };
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 24, w: 600, h: 776 },
                        "win-b": { x: 600, y: 24, w: 600, h: 776 },
                    },
                },
                bounds,
            );
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        let correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 24, w: 600, h: 776 } },
                    { window: "win-b", rect: { x: 600, y: 24, w: 600, h: 776 } },
                ],
                "win-a-leaf",
            ),
        );
        mocks.observeImpl = () =>
            makeObserved3(
                refs,
                {
                    focused: refs.a,
                    rects: {
                        "win-a": { x: 0, y: 24, w: 560, h: 776 },
                        "win-b": { x: 0, y: 0, w: 1200, h: 800 },
                    },
                },
                bounds,
            );
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        const carried = plannerPayload(mocks, 1)["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(
            carried.find((entry) => entry["window"] === "win-b")?.["rect"],
            { x: 600, y: 24, w: 600, h: 776 },
            "the valid applied projection replaces the transient raw frame",
        );
        correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 24, w: 600, h: 776 } },
                    { window: "win-b", rect: { x: 600, y: 24, w: 600, h: 776 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.equal(adapter.isEnabled, true);
    });

    it("prefers fullscreen over maximize for the write disposition", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        twoWindowBaseline(mocks, refs);
        // Admit win-c tiled first so its later fullscreen overlay retains a
        // slot instead of the born-fullscreen hold.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 600, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                    "win-c": { x: 0, y: 0, w: 100, h: 100 },
                },
            });
        fire(mocks, "added");
        runDebounce(mocks);
        const admitCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        // The tiled member takes both overlays at once; the slot is retained.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
                maximized: { "win-c": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        fire(mocks, "fullscreen");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore, "overlay entry keeps its slot without dispatch");
        // Sibling drift reconciles; the doubly overlaid member keeps its slot.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 420, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 0, y: 0, w: 1200, h: 800 },
                },
                fullscreen: { "win-c": true },
                maximized: { "win-c": true },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore + 1);
        const corr = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        const overlayWrites = mocks.geometries.length;
        mocks.callbacks[mocks.dbusCalls.length - 1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.ok(!mocks.geometries.slice(overlayWrites).some((entry) => entry.target === refs.c), "overlay member never actuated");
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line === "omnitiler:plan:write window=win-c resource_class=unknown disposition=skip-fullscreen rect=800,0,400,800",
            ),
            "fullscreen disposition wins when both fullscreen and maximized",
        );
        assert.ok(!mocks.logs.some((line) => line.includes("disposition=skip-maximized")));
    });

    it("entering maximize from tiled adopts the baseline with no reconcile and no write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                maximized: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        const writesBefore = mocks.geometries.length;
        fire(mocks, "maximize");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(mocks.geometries.length, writesBefore);
        assert.equal(mocks.maximizeClears.length, 0, "post-admission maximize is never cleared");
        fire(mocks, "maximize");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(adapter.isEnabled, true);
    });

    it("retile never clears and an immediate re-maximize never loops (R-MAX-03)", () => {
        // R-MAX-03 Q3: first seen on a floating domain, then retiled with
        // maximize kept: no clear runs, the overlay admission applies, and an
        // immediate re-maximize never clears or dispatches a loop.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = false;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = () => tiled;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 600, y: 0, w: 600, h: 800 },
                },
                maximized: { "win-a": true },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 0, "floating startup dispatches nothing");
        assert.equal(mocks.maximizeClears.length, 0, "floating startup clears nothing");
        tiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "retile dispatches one reconcile");
        assert.equal(mocks.maximizeClears.length, 0, "retile issues no native maximize clear");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 800, h: 800 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.ok(
            mocks.logs.some((line) => line.includes(`cmd=${correlation}`) && line.includes("outcome=planned-applied")),
            "overlay admission applies",
        );
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "maximized overlay never actuated");
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "sibling takes its share");
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 400, y: 0, w: 800, h: 800 },
                },
                maximized: { "win-a": true },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        assert.equal(mocks.maximizeClears.length, 0, "post-admission maximize never clears");
        assert.equal(mocks.dbusCalls.length, 1, "post-admission maximize does not dispatch a loop");
        assert.equal(adapter.isEnabled, true);
    });

    it("a re-maximize echo between retile dispatch and reply still applies with no clear", () => {
        // Reply-boundary tolerance without any clear: the retile dispatch
        // observes maximize, a synchronous re-maximize echo lands before the
        // reply with the flag still set, and the admission still applies
        // instead of failing stale-scope.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = false;
        (mocks.env as unknown as { isDomainTiled?: (output: string, workspace: string) => boolean }).isDomainTiled = () => tiled;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 1200, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
                maximized: { "win-a": true },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 0, "floating startup dispatches nothing");
        tiled = true;
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.maximizeClears.length, 0, "retile issues no native maximize clear");
        assert.equal((plannerPayload(mocks, 0)["command"] as Record<string, unknown>)["op"], "reconcile");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        fire(mocks, "maximize", refs.a);
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[0]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 800 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 700, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(
            mocks.logs.some((line) => line.includes(`cmd=${correlation}`) && line.includes("outcome=planned-applied")),
            "re-maximized admission reconcile applies with the flag still set",
        );
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "unaffected sibling still tiled");
        assert.ok(!mocks.geometries.slice(writesBefore).some((entry) => entry.target === refs.a), "re-maximized member never actuated");
        assert.equal(mocks.maximizeClears.length, 0, "no clear at any point");
        assert.equal(adapter.isEnabled, true);
    });

    it("leaving maximize restores the exact retained tiled position via one reconcile", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                maximized: { "win-b": true },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        // KWin restores the window imperfectly: reconcile must reassert the
        // retained slot rather than adopt the post-maximize position.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 100, y: 100, w: 400, h: 400 } },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        const reconcileIndex = mocks.dbusCalls.length - 1;
        assert.deepEqual((plannerPayload(mocks, reconcileIndex)["command"] as Record<string, unknown>)["op"], "reconcile");
        const corr = plannerPayload(mocks, reconcileIndex)["correlation_id"] as string;
        const writesBefore = mocks.geometries.length;
        mocks.callbacks[reconcileIndex]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 600, h: 800 } },
                    { window: "win-b", rect: { x: 600, y: 0, w: 600, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        assert.ok(mocks.geometries.length > writesBefore);
        assert.ok(
            mocks.geometries.some((entry) => entry.target === refs.b && entry.rect.x === 600 && entry.rect.y === 0),
            "exact retained position restored",
        );
        // The reconcile write restored the retained slot: adopt the converged
        // observation and confirm no further dispatch.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 600, y: 0, w: 600, h: 800 } },
            });
        const callsAfter = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsAfter);
        assert.equal(adapter.isEnabled, true);
    });

    it("maximized-only rect drift adopts the baseline without dispatching reconcile", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = twoWindowBaseline(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 0, y: 0, w: 1200, h: 800 } },
                maximized: { "win-b": true },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": { x: 100, y: 50, w: 1000, h: 700 } },
                maximized: { "win-b": true },
            });
        const callsBefore = mocks.dbusCalls.length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore);
        assert.equal(adapter.isEnabled, true);
    });

    it("reflows siblings while one member is maximized without actuating it", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 400, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
            });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const admitCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(
            plannedReply(
                admitCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-a-leaf",
            ),
        );
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
                maximized: { "win-a": true },
            });
        fire(mocks, "maximize");
        runDebounce(mocks);
        // Focus is exempt from maximize isolation: a move on a non-maximized
        // sibling reflows around the maximized member without writing it.
        mocks.observeImpl = () =>
            makeObserved3(refs, {
                focused: refs.b,
                rects: {
                    "win-a": { x: 0, y: 0, w: 1200, h: 800 },
                    "win-b": { x: 400, y: 0, w: 400, h: 800 },
                    "win-c": { x: 800, y: 0, w: 400, h: 800 },
                },
                maximized: { "win-a": true },
            });
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2);
        const moveCorr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(
            plannedReply(
                moveCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 800, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 400, y: 0, w: 400, h: 800 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.b), "sibling reflowed");
        assert.ok(mocks.geometries.some((entry) => entry.target === refs.c), "sibling reflowed");
        assert.ok(!mocks.geometries.some((entry) => entry.target === refs.a), "maximized member not actuated");
        assert.ok(mocks.logs.some((line) => line.includes("kind=move") && line.includes("outcome=planned-applied")));
    });
});

describe("plan adapter observational highlight refresh edge", () => {
    function mockEnvWithApplied(
        refs: { a: object; b: object; c: object },
        applied: string[],
    ): Mocks {
        const mocks = mockEnv(refs);
        (mocks.env as unknown as Record<string, unknown>)["onPlannedApplied"] = (op: unknown): void => {
            applied.push(op as string);
        };
        return mocks;
    }

    function succeedMove(mocks: Mocks, focusLeaf: string): string {
        const correlation = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        mocks.callbacks[mocks.callbacks.length - 1]?.(
            plannedReply(
                correlation,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 500 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 100, h: 100 } },
                ],
                focusLeaf,
            ),
        );
        return correlation;
    }

    it("refreshes exactly once after a successful move even when focus is unchanged", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        mocks.activeImpl = () => refs.a;
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        // Focus leaf matches the already-active window: no focus write, but
        // the geometry plan still applies and must refresh once.
        succeedMove(mocks, "win-a-leaf");
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
        assert.deepEqual(applied, ["move"]);
        assert.equal(adapter.isInFlight, false);
        // Foreground commands are not blocked: the next dispatch proceeds.
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2);
    });

    it("refreshes after resize but never for focus-only plans", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestResize("left", "outwards");
        succeedMove(mocks, "win-a-leaf");
        assert.deepEqual(applied, ["resize"]);
        adapter.requestFocus("right");
        const focusCorr = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        mocks.callbacks[mocks.callbacks.length - 1]?.(
            plannedReply(
                focusCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 500, h: 500 } },
                    { window: "win-b", rect: { x: 500, y: 0, w: 100, h: 100 } },
                ],
                "win-b-leaf",
            ),
        );
        assert.ok(mocks.logs.some((line) => line.includes("kind=focus") && line.includes("outcome=planned-applied")));
        assert.deepEqual(applied, ["resize"]);
    });

    it("never refreshes on rejected or stale boundaries", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        const rejectedCorr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(rejectedReply(rejectedCorr, "snapshot-invalid"));
        assert.deepEqual(applied, []);
        // Stale: a newer debounced observation fences the reply.
        let version = 0;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                fingerprint: version === 0 ? "fp-1" : "fp-2",
                rects:
                    version === 0
                        ? { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } }
                        : { "win-a": { x: 5, y: 5, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } },
            });
        adapter.requestMove("right");
        const staleCorr = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        version = 1;
        fire(mocks, "geometry");
        runDebounce(mocks);
        mocks.callbacks[mocks.callbacks.length - 1]?.(
            plannedReply(
                staleCorr,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 200, h: 200 } },
                    { window: "win-b", rect: { x: 200, y: 0, w: 100, h: 100 } },
                ],
                null,
            ),
        );
        assert.ok(mocks.logs.some((line) => line.includes("outcome=stale-dropped")));
        assert.deepEqual(applied, []);
    });

    it("refreshes after an automatic reconcile that converged a newcomer", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        succeedMove(mocks, "win-a-leaf");
        assert.deepEqual(applied, ["reconcile"], "fresh auto reconcile refreshes like the retired admit edge");
        applied.length = 0;
        // Newcomer win-c with complete observation.
        mocks.observeImpl = () => {
            const wins = [
                Object.freeze({ id: "win-a", ref: refs.a, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
                Object.freeze({ id: "win-b", ref: refs.b, rect: { x: 100, y: 0, w: 500, h: 500 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
                Object.freeze({ id: "win-c", ref: refs.c, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
            ];
            return {
                domainOutput: "out-1",
                domainWorkspace: "ws-1",
                domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                domainGap: DOMAIN_GAP,
                domainOuterGap: OUTER_DOMAIN_GAP,
                focusedId: "win-a",
                windows: Object.freeze(wins),
                activeRef: refs.a,
                fingerprint: "fp-a,b,c",
                revalidate: () => true,
            };
        };
        fire(mocks, "added");
        runDebounce(mocks);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        mocks.callbacks[1]?.(
            plannedReply(
                plannerPayload(mocks, 1)["correlation_id"] as string,
                [
                    { window: "win-a", rect: { x: 0, y: 0, w: 400, h: 800 } },
                    { window: "win-b", rect: { x: 400, y: 0, w: 400, h: 800 } },
                    { window: "win-c", rect: { x: 800, y: 0, w: 400, h: 800 } },
                ],
                "win-c-leaf",
            ),
        );
        assert.deepEqual(applied, ["reconcile"], "membership-changing auto reconcile refreshes");
    });

    it("stays quiet after an automatic reconcile with unchanged membership", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        succeedMove(mocks, "win-a-leaf");
        assert.deepEqual(applied, ["reconcile"]);
        applied.length = 0;
        // Pure client drift: same membership, changed rects.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 200, h: 200 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.deepEqual((plannerPayload(mocks, 1)["command"] as Record<string, unknown>)["op"], "reconcile");
        succeedMove(mocks, "win-a-leaf");
        assert.deepEqual(applied, [], "equal-membership reflow never refreshes");
    });

    it("a throwing refresh hook never wedges foreground commands", () => {
        const refs = makeRefs();
        const applied: string[] = [];
        const mocks = mockEnvWithApplied(refs, applied);
        (mocks.env as unknown as Record<string, unknown>)["onPlannedApplied"] = (): void => {
            throw new Error("hook-failed");
        };
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: {
                    "win-a": { x: 0, y: 0, w: 100, h: 100 },
                    "win-b": { x: 100, y: 0, w: 500, h: 500 },
                },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        succeedMove(mocks, "win-a-leaf");
        assert.ok(mocks.logs.some((line) => line.includes("outcome=planned-applied")));
        assert.deepEqual(applied, []);
        adapter.requestMove("right");
        assert.equal(adapter.isInFlight, true);
    });
});

describe("plan entry workspace-send echo wiring", () => {
    it("binds the send entry to the mover desktopsChanged delayed-arrival signal via signal-capability helpers", () => {
        const srcDir = kwinSrcDir();
        const sendEntry = readFileSync(join(srcDir, "workspace-send-adapter-entry.ts"), "utf8");
        assert.ok(sendEntry.includes("subscribeMoverDesktops"), "mover desktop-change seam");
        assert.ok(sendEntry.includes("desktopsChanged"), "delayed-arrival membership signal");
        assert.ok(sendEntry.includes("connectSignal"), "signal-capability connect");
        assert.ok(sendEntry.includes("readSignal"), "signal-capability read");
        assert.ok(!sendEntry.includes("subscribeWindowGeometry"), "no per-window geometry echo in the send path");
        assert.ok(!sendEntry.includes("frameGeometryChanged"), "no per-window geometry signal in the send path");
        assert.ok(!sendEntry.includes("onGeometryEcho"), "no geometry echo handler in the send path");
        const adapterSrc = readFileSync(join(srcDir, "workspace-send-adapter.ts"), "utf8");
        assert.ok(adapterSrc.includes("subscribeMoverDesktops"), "adapter seam");
        assert.ok(adapterSrc.includes("desktopsChanged"), "delayed-arrival signal reference");
        assert.ok(adapterSrc.includes("armArrivalSignal"), "one-shot arrival arming");
        assert.ok(adapterSrc.includes("onArrivalSignal"), "one-shot arrival handler");
        assert.ok(adapterSrc.includes('"arrival"') && adapterSrc.includes('"waiting"'), "arrival-waiting diagnostic");
        assert.ok(!adapterSrc.includes("subscribeWindowGeometry"), "adapter has no geometry-echo seam");
        assert.ok(!adapterSrc.includes("onGeometryEcho") && !adapterSrc.includes("onMoverEcho"), "no echo handlers");
        assert.ok(!adapterSrc.includes("plan-echo") && !adapterSrc.includes("plan-geometry"), "no echo diagnostics");
    });

    it("exposes a real move-workspace shortcut chord feeding the send transport", () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null);
        const move = mocks.shortcuts.find((row) => row.action === "omnitiler-move-workspace-2");
        assert.ok(move !== undefined, "current Meta+Shift+2 move chord must exist");
        assert.equal(move.sequence, "Meta+Shift+2");
        handle.stop();
    });

    it("supports a native mover desktopsChanged echo through the fake signal seam", () => {
        const world = fakeWorld();
        const mover = world.wins[0] as object;
        const seam = world.winDesktops.get(mover);
        assert.ok(seam !== undefined, "mover must expose a desktopsChanged fake signal");
        let observed = 0;
        const detach = (() => {
            try {
                const signal = (mover as Record<string, unknown>)["desktopsChanged"] as {
                    connect: (handler: () => void) => void;
                    disconnect: (handler: () => void) => void;
                };
                const handler = (): void => {
                    observed += 1;
                };
                signal.connect(handler);
                return (): void => {
                    signal.disconnect(handler);
                };
            } catch (error) {
                void error;
                return null;
            }
        })();
        assert.ok(detach !== null, "fake desktopsChanged must be connectable");
        for (const handler of seam.handlers) {
            handler();
        }
        assert.equal(observed, 1, "echo fires exactly once through the connectable signal");
        detach();
        assert.equal(seam.handlers.length, 0, "one-shot detach releases the echo subscription");
    });
});

describe("plan ordinary lifecycle diagnostics", () => {
    const succA = { x: 0, y: 0, w: 500, h: 500 };
    const succB = { x: 500, y: 0, w: 100, h: 100 };

    function lifecycle(mocks: Mocks): string[] {
        return mocks.logs.filter((l) => l.includes("stage=reply") || l.includes("stage=apply") || l.includes("stage=observe") || l.includes("stage=terminal"));
    }

    function driveMove(mocks: Mocks, refs: { a: object; b: object; c: object }): { adapter: PlanAdapter; correlation: string } {
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } },
            });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        return { adapter, correlation };
    }

    it("emits success phases in order with shared correlation/generation/revision and exact native order", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const { adapter, correlation } = driveMove(mocks, refs);
        const dbusBefore = mocks.dbusCalls.length;
        mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
        assert.equal(mocks.dbusCalls.length, dbusBefore, "no new D-Bus during apply");
        assert.equal(mocks.geometries.length, 2, "both members actuate");
        assert.equal(mocks.geometries[0]?.target, refs.a, "grow-before-shrink writes grower first");
        assert.equal(mocks.geometries[1]?.target, refs.b);
        assert.deepEqual(mocks.actives, [refs.b], "single focus write to resolved target");
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=planned-applied")));
        const lines = lifecycle(mocks);
        const idx = (frag: string): number => lines.findIndex((l) => l.includes(frag));
        const order = ["event=reply outcome=received", "event=validate outcome=validated", "event=observe outcome=matched", "event=apply outcome=started", "event=setters outcome=applied", "event=settled outcome=applied"];
        let last = -1;
        for (const frag of order) {
            const at = idx(frag);
            assert.ok(at > last, `${frag} in order: ${lines.join("\n")}`);
            last = at;
        }
        for (const line of lines) {
            assert.ok(line.includes(`cmd=${correlation}`), line);
            assert.ok(line.includes(`correlation=${correlation}`), line);
            assert.ok(line.includes("generation=gen-1"), line);
            assert.ok(line.includes("component=cosmic-plan"), line);
            assert.ok(line.includes("route=plan"), line);
        }
        assert.ok(lines.some((l) => l.includes("event=reply") && l.includes("revision=0")), "received uses request revision");
        assert.ok(lines.some((l) => l.includes("event=validate") && l.includes("outcome=validated") && l.includes("revision=2")), "validated carries base revision");
        assert.ok(lines.some((l) => l.includes("event=settled") && l.includes("outcome=applied") && l.includes("cause=setters") && l.includes("revision=2")), "terminal applied with last phase setters");
        for (const line of lines) {
            assert.ok(!line.includes("win-a") && !line.includes("win-b"), `no window id leak: ${line}`);
            assert.ok(!line.includes("owner-1"), `no owner leak: ${line}`);
            assert.ok(!line.includes("500,500") && !line.includes("100,100"), `no geometry leak: ${line}`);
        }
        assert.ok(!lines.some((l) => l.includes("ack") || l.includes("verify") || l.includes("verified")), "ordinary route has no ack/verify");
    });

    it("distinguishes malformed, refused, and stale replies with terminal last phase", () => {
        for (const [name, replyOf, validateFrag, terminalFrag] of [
            ["malformed-json", (_c: string): unknown => "{bad", "outcome=malformed", "outcome=uncertain cause=validate"],
            ["refused", (c: string): unknown => rejectedReply(c, "snapshot-invalid"), "outcome=rejected", "outcome=rejected cause=validate"],
            ["correlation-stale", (_c: string): unknown => plannedReply("gen-1-p9999", [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], null), "outcome=stale", "outcome=uncertain cause=validate"],
        ] as const) {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const { adapter, correlation } = driveMove(mocks, refs);
            const geoBefore = mocks.geometries.length;
            mocks.callbacks[0]?.(replyOf(correlation));
            assert.equal(mocks.geometries.length, geoBefore, `${name}: no native writes`);
            assert.equal(adapter.isInFlight, false, `${name}: terminal`);
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=reply") && l.includes("outcome=received")), `${name}: received`);
            assert.ok(lines.some((l) => l.includes("event=validate") && l.includes(validateFrag)), `${name}: ${lines.join("\n")}`);
            assert.ok(lines.some((l) => l.includes("event=settled") && l.includes(terminalFrag)), `${name}: ${lines.join("\n")}`);
        }
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            let version = 0;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    fingerprint: version === 0 ? "fp-1" : "fp-2",
                    rects: version === 0
                        ? { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } }
                        : { "win-a": { x: 5, y: 5, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } },
                });
            const adapter = enableAdapter(mocks);
            adapter.requestFocus("left");
            const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
            version = 1;
            fire(mocks, "geometry");
            runDebounce(mocks);
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], null));
            assert.equal(mocks.geometries.length, 0, "epoch-stale never writes");
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=validate") && l.includes("outcome=stale")), lines.join("\n"));
            assert.ok(lines.some((l) => l.includes("event=settled") && l.includes("outcome=uncertain") && l.includes("cause=validate")), lines.join("\n"));
            assert.equal(adapter.isEnabled, true);
        }
    });

    it("quiet-equal ticks never stale an in-flight reply while real drift still stales", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const { adapter, correlation: first } = driveMove(mocks, refs);
        mocks.callbacks[0]?.(plannedReply(first, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
        assert.equal(mocks.geometries.length, 2, "seed apply writes");
        assert.equal(adapter.isInFlight, false);
        // Windows have landed on the planned geometry: further identical
        // observations are proven quiet-equal against applied evidence.
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": { ...succA }, "win-b": { ...succB } },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "landed geometry stays quiet");
        // Second flight from the landed state; its snapshot equals applied.
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2, "second move dispatches");
        const quietCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
        assert.equal(adapter.isInFlight, true);
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "quiet tick dispatches nothing while in-flight");
        assert.equal(adapter.isInFlight, true, "quiet tick holds the flight");
        mocks.callbacks[1]?.(plannedReply(quietCorrelation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
        assert.equal(adapter.isInFlight, false, "quiet-equal lets the in-flight reply settle");
        assert.ok(mocks.logs.some((l) => l.includes(`cmd=${quietCorrelation}`) && l.includes("outcome=planned-applied")), "quiet survivor applies");
        assert.ok(!mocks.logs.some((l) => l.includes(`cmd=${quietCorrelation}`) && l.includes("outcome=stale")), "quiet survivor never stales");
        // Real drift while in-flight must still stale the superseded reply.
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 3, "third move dispatches");
        const driftCorrelation = plannerPayload(mocks, 2)["correlation_id"] as string;
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                fingerprint: "fp-drift",
                rects: { "win-a": { x: 5, y: 5, w: 500, h: 500 }, "win-b": { ...succB } },
            });
        fire(mocks, "geometry");
        runDebounce(mocks);
        mocks.callbacks[2]?.(plannedReply(driftCorrelation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
        assert.equal(mocks.geometries.length, 2, "real drift still stales the old reply");
        const lines = lifecycle(mocks);
        assert.ok(lines.some((l) => l.includes("event=validate") && l.includes("outcome=stale")), lines.join("\n"));
    });

    it("fails one setter logging-only with unchanged order/count and terminal uncertain", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const { adapter, correlation } = driveMove(mocks, refs);
        mocks.geometryImpl = (target): boolean => target !== refs.b;
        mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
        assert.equal(mocks.geometries.length, 2, "both setters attempted in order");
        assert.equal(mocks.geometries[0]?.target, refs.a);
        assert.equal(mocks.geometries[1]?.target, refs.b);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=write-failed")));
        const lines = lifecycle(mocks);
        assert.ok(lines.some((l) => l.includes("event=setters") && l.includes("outcome=write-failed")), lines.join("\n"));
        assert.ok(lines.some((l) => l.includes("event=settled") && l.includes("outcome=uncertain") && l.includes("cause=setters")), lines.join("\n"));
        assert.equal(adapter.isInFlight, false);
        assert.ok(!mocks.logs.some((l) => l.includes("outcome=planned-applied") && l.includes(`cmd=${correlation}`)));
    });

    it("aggregates fullscreen, maximize, and float skips without per-window normal geometry", () => {
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a });
            const adapter = enableAdapter(mocks);
            // Seed win-b as a known tiled member so its later fullscreen
            // overlay retains a slot instead of the born-fullscreen hold.
            fire(mocks, "added");
            runTimers(mocks);
            const seed = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.callbacks[0]?.(rejectedReply(seed, "snapshot-invalid"));
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, fullscreen: { "win-b": true } });
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
            mocks.callbacks[1]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-a-leaf"));
            assert.ok(!mocks.geometries.some((g) => g.target === refs.b), "fullscreen never actuated");
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=setters") && l.includes("outcome=applied") && l.includes("skipped-fullscreen")), lines.join("\n"));
            assert.equal(adapter.isInFlight, false);
        }
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, maximized: { "win-b": true } });
            const adapter = enableAdapter(mocks);
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-a-leaf"));
            assert.ok(!mocks.geometries.some((g) => g.target === refs.b), "maximized never actuated");
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=setters") && l.includes("outcome=applied") && l.includes("skipped-maximized")), lines.join("\n"));
            assert.equal(adapter.isInFlight, false);
        }
        {
            // Mid-flight floating drift replans once (resilience row G): the
            // move was planned with win-b tiled, so the stale reply writes
            // nothing; the same command dispatches again against the fresh
            // floating observation with a new correlation. A second staleness
            // drops and converges normally with no third dispatch.
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a });
            const adapter = enableAdapter(mocks);
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, floating: { "win-b": true } });
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-a-leaf"));
            assert.equal(mocks.geometries.length, 0, "stale flight writes nothing after a mid-flight float");
            assert.equal(mocks.actives.length, 0);
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=observe") && l.includes("outcome=mismatched") && l.includes("stale-scope")), lines.join("\n"));
            assert.equal(mocks.dbusCalls.length, 2, "stale pre-write replans once");
            const replan = plannerPayload(mocks, 1);
            const replanCorrelation = replan["correlation_id"] as string;
            assert.notEqual(replanCorrelation, correlation, "replan uses a fresh correlation");
            assert.deepEqual(replan["command"], { op: "move", window: "win-a", direction: "right" }, "replan carries the same command");
            const replanWindows = replan["windows"] as Array<Record<string, unknown>>;
            assert.equal((replanWindows.find((entry) => entry["window"] === "win-b") as Record<string, unknown>)["floating"], true, "replan carries fresh floating evidence");
            const replanLine = mocks.logs.find((l) => l.includes(":stale-replan") && l.includes(`correlation=${correlation}`));
            assert.ok(replanLine?.includes("cause=stale-scope") && replanLine?.includes("recovery=replan-once"), mocks.logs.join("\n"));
            assert.ok(!replanLine?.includes("win-a") && !replanLine?.includes("win-b"), `no window id leak: ${replanLine}`);
            assert.equal(adapter.isInFlight, true, "replanned command is in flight");
            // The replanned move converges: only win-a stays wanted once
            // win-b floats, so the covering reply carries win-a alone.
            mocks.callbacks[1]?.(plannedReply(replanCorrelation, [{ window: "win-a", rect: succA }], "win-a-leaf"));
            assert.equal(mocks.geometries.length, 1, "replanned command applies against fresh observation");
            assert.equal(mocks.geometries[0]?.target, refs.a);
            assert.ok(mocks.logs.some((l) => l.includes("outcome=planned-applied") && l.includes(`cmd=${replanCorrelation}`)));
            assert.equal(adapter.isInFlight, false);
            assert.equal(adapter.isEnabled, true);
        }
        {
            // Second staleness drops: the replanned flight observes another
            // drift, fails closed with stale-scope, and dispatches nothing
            // further.
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a });
            const adapter = enableAdapter(mocks);
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, floating: { "win-b": true } });
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-a-leaf"));
            assert.equal(mocks.dbusCalls.length, 2, "first staleness replans");
            const replanCorrelation = plannerPayload(mocks, 1)["correlation_id"] as string;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    floating: { "win-b": true },
                    rects: { "win-a": { x: 7, y: 7, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 100, h: 100 } },
                    fingerprint: "fp-2",
                });
            mocks.callbacks[1]?.(plannedReply(replanCorrelation, [{ window: "win-a", rect: succA }], "win-a-leaf"));
            assert.equal(mocks.geometries.length, 0, "second staleness writes nothing");
            assert.equal(mocks.dbusCalls.length, 2, "second staleness dispatches nothing further");
            const lines = lifecycle(mocks);
            assert.equal(lines.filter((l) => l.includes("event=observe") && l.includes("outcome=mismatched")).length, 2, lines.join("\n"));
            assert.ok(mocks.logs.some((l) => l.includes(`cmd=${replanCorrelation}`) && l.includes("outcome=stale-scope")), mocks.logs.join("\n"));
            assert.equal(adapter.isInFlight, false, "dropped flight converges");
            assert.equal(adapter.isEnabled, true);
        }
    });

    it("aggregates skipped-equal and skipped-mixed in one bounded setters line", () => {
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } },
                });
            const adapter = enableAdapter(mocks);
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.callbacks[0]?.(
                plannedReply(
                    correlation,
                    [
                        { window: "win-a", rect: { x: 0, y: 0, w: 100, h: 100 } },
                        { window: "win-b", rect: { x: 100, y: 0, w: 400, h: 500 } },
                    ],
                    "win-b-leaf",
                ),
            );
            assert.equal(mocks.geometries.length, 1, "unchanged member skipped, changed member written");
            assert.equal(mocks.geometries[0]?.target, refs.b);
            const lines = lifecycle(mocks);
            const setters = lines.filter((l) => l.includes("event=setters"));
            assert.equal(setters.length, 1, `exactly one aggregate setters line: ${lines.join("\n")}`);
            assert.ok(setters[0]?.includes("outcome=applied") && setters[0]?.includes("cause=skipped-equal"), setters.join("\n"));
            assert.equal(adapter.isInFlight, false);
        }
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } },
                });
            const adapter = enableAdapter(mocks);
            // Seed win-b as a known tiled member so its later fullscreen
            // overlay retains a slot instead of the born-fullscreen hold.
            fire(mocks, "added");
            runTimers(mocks);
            const seed = plannerPayload(mocks, 0)["correlation_id"] as string;
            mocks.callbacks[0]?.(rejectedReply(seed, "snapshot-invalid"));
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.a,
                    rects: { "win-a": { x: 0, y: 0, w: 100, h: 100 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } },
                    fullscreen: { "win-b": true },
                });
            adapter.requestMove("right");
            const correlation = plannerPayload(mocks, 1)["correlation_id"] as string;
            mocks.callbacks[1]?.(
                plannedReply(
                    correlation,
                    [
                        { window: "win-a", rect: { x: 0, y: 0, w: 100, h: 100 } },
                        { window: "win-b", rect: { x: 100, y: 0, w: 400, h: 500 } },
                    ],
                    "win-a-leaf",
                ),
            );
            assert.ok(!mocks.geometries.some((g) => g.target === refs.b), "fullscreen never actuated");
            assert.equal(mocks.geometries.length, 0, "unchanged member skipped, fullscreen skipped: no writes");
            const lines = lifecycle(mocks);
            const setters = lines.filter((l) => l.includes("event=setters"));
            assert.equal(setters.length, 1, `exactly one aggregate setters line: ${lines.join("\n")}`);
            assert.ok(setters[0]?.includes("outcome=applied") && setters[0]?.includes("cause=skipped-mixed"), setters.join("\n"));
            assert.equal(adapter.isInFlight, false);
        }
    });

    it("covers observation mismatch and timeout with last phase and no ack/verify", () => {
        {
            // Pre-write rect drift replans the same move once (resilience row
            // G): the stale reply writes nothing, the same command dispatches
            // again with fresh geometry, and the covering second reply
            // applies.
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const { adapter, correlation } = driveMove(mocks, refs);
            const firstCommand = plannerPayload(mocks, 0)["command"];
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, fingerprint: "fp-other", rects: { "win-a": { x: 9, y: 9, w: 50, h: 50 }, "win-b": { x: 100, y: 0, w: 500, h: 500 } } });
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
            assert.equal(mocks.geometries.length, 0, "stale pre-write writes nothing");
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("stage=observe") && l.includes("outcome=mismatched")), lines.join("\n"));
            assert.ok(lines.some((l) => l.includes("stage=terminal") && l.includes("outcome=uncertain") && l.includes("cause=observe")), lines.join("\n"));
            assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-scope")));
            assert.equal(mocks.dbusCalls.length, 2, "stale pre-write replans once");
            const replan = plannerPayload(mocks, 1);
            assert.deepEqual(replan["command"], firstCommand, "replan carries the same command");
            assert.notEqual(replan["correlation_id"], correlation, "replan uses a fresh correlation");
            const replanCorrelation = replan["correlation_id"] as string;
            assert.equal(adapter.isInFlight, true, "replanned command is in flight");
            mocks.callbacks[1]?.(plannedReply(replanCorrelation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
            assert.equal(mocks.geometries.length, 2, "replanned command applies against fresh observation");
            assert.ok(mocks.logs.some((l) => l.includes("outcome=planned-applied") && l.includes(`cmd=${replanCorrelation}`)));
            assert.equal(adapter.isInFlight, false);
        }
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const { adapter, correlation } = driveMove(mocks, refs);
            runTimers(mocks);
            assert.equal(mocks.geometries.length, 0);
            const lines = lifecycle(mocks);
            const terminals = lines.filter((l) => l.includes("stage=terminal"));
            assert.equal(terminals.length, 1, `exactly one ordinary terminal: ${lines.join("\n")}`);
            assert.equal(terminals[0], `omnitiler:plan:cmd=${correlation} kind=move windows=2 component=cosmic-plan route=plan stage=terminal correlation=${correlation} generation=gen-1 revision=0 event=settled outcome=timeout cause=reply`);
            assert.ok(mocks.logs.some((l) => l.includes("outcome=timeout")));
            assert.ok(!mocks.logs.some((l) => l.includes("ack") || l.includes("verify") || l.includes("verified")), "timeout path has no ack/verify");
            assert.equal(adapter.isInFlight, false);
            void correlation;
        }
    });

    it("emits no ordinary terminal for activation-phase timeouts", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const held: Array<(reply: unknown) => void> = [];
        const origCall = mocks.env.callDbus;
        (mocks.env as { callDbus: PlanAdapterEnv["callDbus"] }).callDbus = ((service, path, iface, method, payload, callback): void => {
            if (method === "NameHasOwner") {
                held.push(callback);
                return;
            }
            return origCall(service, path, iface, method, payload, callback);
        }) as PlanAdapterEnv["callDbus"];
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        assert.equal(held.length, 1, "presence lookup held");
        runTimers(mocks);
        assert.equal(mocks.geometries.length, 0);
        assert.ok(mocks.logs.some((l) => l.includes("stage=activate") && l.includes("event=timeout") && l.includes("outcome=timeout")), mocks.logs.join("\n"));
        assert.ok(mocks.logs.some((l) => l.includes("outcome=timeout")));
        assert.ok(!lifecycle(mocks).some((l) => l.includes("stage=terminal")), `activation timeout stays outside ordinary records: ${mocks.logs.join("\n")}`);
        assert.equal(adapter.isInFlight, false);
    });

    it("settles an R4-shape move without transfer capabilities via ordinary timeout, with no cancel records", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const bounds = { x: 0, y: 0, w: 1200, h: 800 };
        const domains = Object.freeze([
            Object.freeze({ output: "out-1", workspace: "ws-1", bounds, gap: DOMAIN_GAP, outerGap: OUTER_DOMAIN_GAP, adjacent: Object.freeze({ right: "out-2" }) }),
            Object.freeze({ output: "out-2", workspace: "ws-2", bounds, gap: DOMAIN_GAP, outerGap: OUTER_DOMAIN_GAP, adjacent: Object.freeze({ left: "out-1" }) }),
        ]);
        const twoDomain = (): PlanObserved => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-1",
            domainBounds: bounds,
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: "win-a",
            domains,
            windows: Object.freeze([
                Object.freeze({ id: "win-a", ref: refs.a, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-1", workspace: "ws-1", fullscreen: false, maximized: false }),
                Object.freeze({ id: "win-x", ref: refs.b, rect: { x: 0, y: 0, w: 100, h: 100 }, output: "out-2", workspace: "ws-2", fullscreen: false, maximized: false }),
            ]),
            activeRef: refs.a,
            fingerprint: "fp-r4",
            revalidate: () => true,
        });
        mocks.observeImpl = twoDomain;
        (mocks.env as { observeDirectional?: (direction: PlanDirection) => DirectionalObservation }).observeDirectional = () => ({ status: "ready", observed: twoDomain() });
        const adapter = enableAdapter(mocks);
        adapter.requestMove("right");
        const correlation = plannerPayload(mocks, 0)["correlation_id"] as string;
        runTimers(mocks);
        assert.ok(!mocks.logs.some((l) => l.includes("stage=cancel")), `wire cancel removed, no cancel records: ${mocks.logs.join("\n")}`);
        assert.ok(!mocks.logs.some((l) => l.includes("cancel-requested")), `no cancel attempts: ${mocks.logs.join("\n")}`);
        assert.ok(!lifecycle(mocks).some((l) => l.includes("stage=reply")), "no ordinary reply record without a reply");
        assert.ok(lifecycle(mocks).some((l) => l.includes(`cmd=${correlation}`) && l.includes("stage=terminal") && l.includes("outcome=timeout")), `ordinary timeout owns the flight: ${mocks.logs.join("\n")}`);
        assert.equal(mocks.geometries.length, 0, "no native writes without transfer capabilities");
        assert.equal(adapter.isInFlight, false, "unanswered R4-shape flight releases");
        adapter.disable();
    });

    it("keeps duplicate and late callbacks inert without acceptance or completion", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const { adapter, correlation } = driveMove(mocks, refs);
        const reply = plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf");
        mocks.callbacks[0]?.(reply);
        const lifecycleBefore = lifecycle(mocks).length;
        const logsBefore = mocks.logs.length;
        const geoCount = mocks.geometries.length;
        mocks.callbacks[0]?.(reply);
        assert.equal(mocks.geometries.length, geoCount, "duplicate never rewrites");
        assert.equal(lifecycle(mocks).length, lifecycleBefore, "duplicate emits no lifecycle record at all");
        assert.equal(mocks.logs.length, logsBefore, "duplicate emits no log line at all");
        assert.equal(adapter.isInFlight, false);
    });

    it("carries unavailable revision truthfully and survives logger throws", () => {
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const { correlation } = driveMove(mocks, refs);
            const noRev = JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "planned",
                desired_geometry: [
                    { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: succA },
                    { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: succB },
                ],
            });
            mocks.callbacks[0]?.(noRev);
            const lines = lifecycle(mocks);
            assert.ok(lines.some((l) => l.includes("event=validate") && l.includes("outcome=validated") && l.includes("revision=unavailable")), lines.join("\n"));
            assert.ok(lines.some((l) => l.includes("event=settled") && l.includes("outcome=applied") && l.includes("revision=unavailable")), lines.join("\n"));
        }
        {
            const refs = makeRefs();
            const mocks = mockEnv(refs);
            const { adapter, correlation } = driveMove(mocks, refs);
            const origLog = mocks.env.log;
            (mocks.env as { log: (message: string) => void }).log = (): void => {
                throw new Error("log-boom");
            };
            mocks.callbacks[0]?.(plannedReply(correlation, [{ window: "win-a", rect: succA }, { window: "win-b", rect: succB }], "win-b-leaf"));
            assert.equal(mocks.geometries.length, 2, "throws never change native count");
            assert.equal(mocks.geometries[0]?.target, refs.a);
            assert.equal(mocks.geometries[1]?.target, refs.b);
            assert.deepEqual(mocks.actives, [refs.b]);
            assert.equal(adapter.isInFlight, false, "throws never wedge flight");
            (mocks.env as { log: (message: string) => void }).log = origLog;
        }
    });
});

describe("plan entry sticky workspace-switch regression", () => {
    // Decision 2026-09-25: Meta+G on a sticky window returns it
    // to tiling ON THE CURRENT WORKSPACE where sticky-off leaves it,
    // regardless of the prior workspace. Production KWin observer
    // (startPlanAdapterEntry with observeNative on the fake world) against
    // the real Engine (planner_eval: same validation and retained state as
    // the shipped service, only D-Bus stubbed). Two KWin-native effects the
    // fake world cannot emit are emulated explicitly and marked below:
    // sticky-off re-homes the window onto the current desktop, and
    // desktopsChanged echoes are fired by hand. The scope-change signal on a
    // workspace switch is not fired; the switch takes effect on the next
    // request's fresh observation, which is exactly the Meta+G ingress under
    // test. Each row uses a fresh Engine so retained sessions never leak
    // across rows.
    type StickyHandle = NonNullable<ReturnType<typeof startPlanAdapterEntry>>;

    function stickyEngineRoot(): string {
        let dir = resolve(process.cwd());
        for (let depth = 0; depth < 4; depth += 1) {
            try {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
            } catch (error) {
                void error;
            }
            dir = dirname(dir);
        }
        throw new Error("fixture root not found");
    }

    class StickyEngineBridge {
        private readonly proc: ChildProcess;
        private readonly waiters: Array<{
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }> = [];
        private dead: string | null = null;

        private constructor(proc: ChildProcess) {
            this.proc = proc;
            const stdout = proc.stdout;
            if (stdout === null || stdout === undefined) {
                throw new Error("planner_eval stdout unavailable");
            }
            createInterface({ input: stdout }).on("line", (line: string) => {
                this.waiters.shift()?.resolve(line);
            });
            proc.stderr?.resume();
            proc.on("error", (error) => {
                this.failAll(error instanceof Error ? error : new Error(String(error)));
            });
            proc.on("exit", (code) => {
                this.failAll(new Error(`planner_eval exited with code ${String(code)}`));
            });
        }

        private failAll(error: Error): void {
            if (this.dead === null) {
                this.dead = error.message;
            }
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(error);
            }
        }

        private deathReason(): string | null {
            if (this.dead !== null) {
                return this.dead;
            }
            if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
                return "planner_eval process already exited";
            }
            return null;
        }

        static start(): StickyEngineBridge {
            return new StickyEngineBridge(
                spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                    cwd: stickyEngineRoot(),
                    stdio: ["pipe", "pipe", "pipe"],
                }),
            );
        }

        send(request: string): Promise<string> {
            const dead = this.deathReason();
            if (dead !== null) {
                return Promise.reject(new Error(dead));
            }
            return new Promise<string>((resolve, reject) => {
                this.waiters.push({ resolve, reject });
                try {
                    this.proc.stdin?.write(request + "\n");
                } catch (error) {
                    reject(error instanceof Error ? error : new Error(String(error)));
                }
            });
        }

        async close(): Promise<void> {
            if (this.deathReason() !== null) {
                try {
                    this.proc.stdin?.destroy();
                } catch (error) {
                    void error;
                }
                return;
            }
            try {
                this.proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolve) => {
                const timer = setTimeout(() => {
                    try {
                        this.proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolve();
                }, 3000);
                this.proc.on("exit", () => {
                    clearTimeout(timer);
                    resolve();
                });
            });
        }
    }

    async function flushPlan(mocks: EntryMocks, engine: StickyEngineBridge, index: number): Promise<Record<string, unknown>> {
        const call = mocks.dbusCalls[index];
        assert.ok(call !== undefined, `dispatch ${index} exists before flushing to the real Engine`);
        const replyText = await engine.send(call.payload);
        mocks.callbacks[index]?.(replyText);
        return JSON.parse(replyText) as Record<string, unknown>;
    }

    function fireGeometry(world: FakeWorld, win: Record<string, unknown>): void {
        const signal = world.winGeometry.get(win);
        assert.ok(signal !== undefined, "frameGeometryChanged subscription present");
        for (const handler of [...signal.handlers]) {
            handler();
        }
    }

    function runEntryDebounce(mocks: EntryMocks): void {
        const pending = [...mocks.timers];
        mocks.timers.length = 0;
        for (const timer of pending) {
            if (timer.cancelled) {
                continue;
            }
            if (timer.delayMs === PLAN_DEBOUNCE_MS) {
                timer.callback();
            } else {
                mocks.timers.push(timer);
            }
        }
    }

    function tailLogs(mocks: EntryMocks): string {
        return mocks.logs.slice(-6).join(" | ");
    }

    // Shared ingress: tiled A -> sticky-on -> switch current desktop to B ->
    // Meta+G clears native sticky with KWin's synchronous re-home and echo,
    // tiling re-invoked on B and flushed through the real Engine.
    // Intermediate replies are returned unasserted so a later fix may change
    // them; rows assert the terminal behavior.
    async function driveStickyToSwitchedWorkspace(
        world: FakeWorld,
        mocks: EntryMocks,
        handle: StickyHandle,
        engine: StickyEngineBridge,
        d2: object,
        firstIndex = 0,
        deliverSwitch = true,
    ): Promise<{ winA: Record<string, unknown>; floatReply: Record<string, unknown>; switchReply: Record<string, unknown> | null; switchCorrelation: string }> {
        const winA = world.wins[0] as Record<string, unknown>;
        assert.ok(winA !== undefined, "win-a present");
        const current: { desktop: object } = { desktop: world.desktop };
        world.workspace["currentDesktopForScreen"] = (): unknown => current.desktop;
        let sticky = winA["onAllDesktops"] === true;
        Object.defineProperty(winA, "onAllDesktops", {
            get: (): boolean => sticky,
            set: (value: unknown): void => {
                sticky = value === true;
                if (!sticky) {
                    winA["desktops"] = [current.desktop];
                }
                const signal = world.winDesktops.get(winA);
                assert.ok(signal !== undefined, "desktopsChanged subscription present for the echo");
                for (const handler of [...signal.handlers]) {
                    handler();
                }
            },
            enumerable: true,
            configurable: true,
        });
        handle.requestSticky();
        assert.equal(mocks.dbusCalls.length, firstIndex + 1, `sticky-on floats the tiled member first, logs: ${tailLogs(mocks)}`);
        const floatReply = await flushPlan(mocks, engine, firstIndex);
        assert.equal(floatReply["outcome"], "planned", `setup: sticky-on float plans, engine replied ${JSON.stringify(floatReply)}`);
        assert.equal(winA["onAllDesktops"], true, "setup: sticky-on takes native all-desktops");
        assert.ok(mocks.logs.includes("omnitiler:plan:sticky-echo-consumed"), "setup: sticky-on echo consumed synchronously");
        current.desktop = d2;
        handle.requestFloat();
        assert.equal(winA["onAllDesktops"], false, "Meta+G clears native all-desktops on the current workspace");
        assert.deepEqual(winA["desktops"], [d2], "KWin re-homes sticky-off onto the current desktop");
        assert.ok(
            mocks.logs.filter((line) => line === "omnitiler:plan:sticky-echo-consumed").length >= 2,
            `sticky-off echo consumed synchronously, logs: ${tailLogs(mocks)}`,
        );
        assert.equal(mocks.dbusCalls.length, firstIndex + 2, `sticky-off re-invokes tiling on the current workspace, logs: ${tailLogs(mocks)}`);
        const payload = JSON.parse(mocks.dbusCalls[firstIndex + 1]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["workspace"], "ws-2", "tiling dispatch targets the current workspace");
        assert.deepEqual((payload["command"] as Record<string, unknown>)["op"], "toggle-float");
        const switchCorrelation = payload["correlation_id"] as string;
        const switchReply = deliverSwitch ? await flushPlan(mocks, engine, firstIndex + 1) : null;
        return { winA, floatReply, switchReply, switchCorrelation };
    }

    function assertTiledOnWorkspace(
        world: FakeWorld,
        mocks: EntryMocks,
        winA: Record<string, unknown>,
        d2: object,
        reply: Record<string, unknown>,
        label: string,
    ): void {
        const desired = reply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
        assert.ok(Array.isArray(desired), `${label}: planned reply carries desired_geometry, got ${JSON.stringify(reply)}`);
        const home = desired.find((entry) => entry["window"] === "win-a");
        assert.ok(home !== undefined, `${label}: planned geometry homes win-a, got ${JSON.stringify(reply)}`);
        assert.equal(home["workspace"], "ws-2", `${label}: tile homes win-a on the current workspace`);
        const rect = home["rect"] as { x: number; y: number; w: number; h: number };
        assert.deepEqual(
            winA["frameGeometry"],
            { x: rect["x"], y: rect["y"], width: rect["w"], height: rect["h"] },
            `${label}: tile rect applied natively`,
        );
        assert.equal(winA["onAllDesktops"], false, `${label}: tiled window leaves native sticky`);
        assert.deepEqual(winA["desktops"], [d2], `${label}: tiled window homed on the current desktop`);
        const calls = mocks.dbusCalls.length;
        fireGeometry(world, winA);
        const winB = world.wins[1] as Record<string, unknown>;
        fireGeometry(world, winB);
        runEntryDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, calls, `${label}: next complete observation stays tiled with no further dispatch`);
    }

    // Floating-state probe through observable request shape: a tiled window
    // floats (command without float_rect, float_geometry present, window
    // absent from tiled geometry) and the next Meta+G unfloats (command with
    // the live float_rect, tiled result applied). Ends tiled and stable.
    async function assertFloatProbeCycle(
        world: FakeWorld,
        mocks: EntryMocks,
        handle: StickyHandle,
        engine: StickyEngineBridge,
        d2: object,
        baseIndex: number,
        label: string,
    ): Promise<void> {
        const winA = world.wins[0] as Record<string, unknown>;
        handle.requestFloat();
        assert.equal(mocks.dbusCalls.length, baseIndex + 1, `${label}: tiled window floats, logs: ${tailLogs(mocks)}`);
        const floatCmd = (JSON.parse(mocks.dbusCalls[baseIndex]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.deepEqual(floatCmd, { op: "toggle-float", window: "win-a" }, `${label}: float dispatch carries no float_rect`);
        const floatReply = await flushPlan(mocks, engine, baseIndex);
        assert.equal(floatReply["outcome"], "planned", `${label}: float plans, engine replied ${JSON.stringify(floatReply)}`);
        const floatGeometry = floatReply["float_geometry"] as Record<string, unknown> | undefined;
        assert.ok(floatGeometry !== undefined && floatGeometry["window"] === "win-a", `${label}: float reply carries float_geometry for win-a`);
        const floatDesired = floatReply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
        assert.ok(
            Array.isArray(floatDesired) && !floatDesired.some((entry) => entry["window"] === "win-a"),
            `${label}: floated window absent from tiled geometry, got ${JSON.stringify(floatReply)}`,
        );
        const floatRect = floatGeometry["rect"] as { x: number; y: number; w: number; h: number };
        assert.deepEqual(
            winA["frameGeometry"],
            { x: floatRect["x"], y: floatRect["y"], width: floatRect["w"], height: floatRect["h"] },
            `${label}: float rect applied natively`,
        );
        handle.requestFloat();
        assert.equal(mocks.dbusCalls.length, baseIndex + 2, `${label}: Meta+G re-invokes tiling, logs: ${tailLogs(mocks)}`);
        const unfloatCmd = (JSON.parse(mocks.dbusCalls[baseIndex + 1]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.ok("float_rect" in unfloatCmd, `${label}: unfloat dispatch carries the live float_rect, got ${JSON.stringify(unfloatCmd)}`);
        const tileReply = await flushPlan(mocks, engine, baseIndex + 1);
        assert.equal(tileReply["outcome"], "planned", `${label}: unfloat plans, engine replied ${JSON.stringify(tileReply)}`);
        assertTiledOnWorkspace(world, mocks, winA, d2, tileReply, `${label} unfloat`);
    }

    it("sticky Meta+G after a workspace switch tiles on the current workspace and stays tiled", async () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            const d2 = { id: "ws-2" };
            const { winA, switchReply } = await driveStickyToSwitchedWorkspace(
                world,
                mocks,
                resolved,
                engine,
                d2,
            );
            assert.equal(
                switchReply?.["outcome"],
                "planned",
                `Meta+G after switch tiles on the current workspace, engine replied ${JSON.stringify(switchReply)}, logs: ${tailLogs(mocks)}`,
            );
            assert.ok(switchReply !== null);
            assertTiledOnWorkspace(world, mocks, winA, d2, switchReply, "switched sticky return");
            await assertFloatProbeCycle(world, mocks, resolved, engine, d2, 2, "switched sticky return");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("plain float cycle after a switched sticky return tiles on the current workspace", async () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            const d2 = { id: "ws-2" };
            const { winA, switchReply } = await driveStickyToSwitchedWorkspace(world, mocks, resolved, engine, d2);
            assert.equal(
                switchReply?.["outcome"],
                "planned",
                `first switched Meta+G tiles, engine replied ${JSON.stringify(switchReply)}, logs: ${tailLogs(mocks)}`,
            );
            assert.ok(switchReply !== null);
            assertTiledOnWorkspace(world, mocks, winA, d2, switchReply, "first switched Meta+G");
            await assertFloatProbeCycle(world, mocks, resolved, engine, d2, 2, "plain float cycle");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    // Extra tiled member for the B-survivor row: mirrors the fake world's
    // own window shape (including the signals enable requires) and must be
    // added before startEntry so subscriptions bind it.
    function addTiledWindow(world: FakeWorld, id: string, desktop: object, x: number): Record<string, unknown> {
        const geo = fakeSignal();
        const full = fakeSignal();
        const max = fakeSignal();
        const desk = fakeSignal();
        const interactive = fakeSignal();
        const win: Record<string, unknown> = {
            normalWindow: true,
            internalId: id,
            resourceClass: "test-app",
            output: world.output,
            desktops: [desktop],
            frameGeometry: { x, y: 0, width: 600, height: 800 },
            frameGeometryChanged: geo.signal,
            moveResizedChanged: interactive.signal,
            fullScreenChanged: full.signal,
            fullScreen: false,
            maximizedChanged: max.signal,
            maximizeMode: 0,
            desktopsChanged: desk.signal,
            onAllDesktops: false,
            keepAbove: false,
            keepBelow: false,
        };
        win["setMaximize"] = (vertically: unknown, horizontally: unknown): void => {
            if (vertically !== false || horizontally !== false) {
                return;
            }
            world.maximizeClears.push(win);
            win["maximizeMode"] = 0;
            for (const handler of max.handlers) {
                handler();
            }
        };
        world.winFull.set(win, full);
        world.winMax.set(win, max);
        world.winDesktops.set(win, desk);
        world.winGeometry.set(win, geo);
        world.winInteractiveGeometry.set(win, interactive);
        world.wins.push(win);
        return win;
    }

    it("switched sticky return converges into a retained B session without displacing the B survivor", async () => {
        // Bounded: B already holds tiled win-c with a retained Engine
        // session, A keeps win-b. Meta+G must tile the floating newcomer on
        // B while win-c keeps its placement and no A topology relocates.
        const world = fakeWorld();
        const d2 = { id: "ws-2" };
        const winC = addTiledWindow(world, "win-c", d2, 600);
        const winB = world.wins[1] as Record<string, unknown>;
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            world.workspace["currentDesktopForScreen"] = (): unknown => d2;
            world.workspace["activeWindow"] = winC;
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 1, `B session seeds with a float, logs: ${tailLogs(mocks)}`);
            const seedFloat = await flushPlan(mocks, engine, 0);
            assert.equal(seedFloat["outcome"], "planned", `setup: B float plans, engine replied ${JSON.stringify(seedFloat)}`);
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 2, `B session restores its tile, logs: ${tailLogs(mocks)}`);
            const seedTile = await flushPlan(mocks, engine, 1);
            assert.equal(seedTile["outcome"], "planned", `setup: B unfloat plans, engine replied ${JSON.stringify(seedTile)}`);
            world.workspace["activeWindow"] = world.wins[0];
            const { winA, floatReply, switchReply } = await driveStickyToSwitchedWorkspace(world, mocks, resolved, engine, d2, 2);
            assert.equal(
                switchReply?.["outcome"],
                "planned",
                `Meta+G converges the floating newcomer on B, engine replied ${JSON.stringify(switchReply)}, logs: ${tailLogs(mocks)}`,
            );
            assert.ok(switchReply !== null);
            assertTiledOnWorkspace(world, mocks, winA, d2, switchReply, "B newcomer return");
            const tiled = switchReply["desired_geometry"] as Array<Record<string, unknown>>;
            const survivor = tiled.find((entry) => entry["window"] === "win-c");
            assert.ok(survivor !== undefined, `B survivor stays a tiled member, got ${JSON.stringify(switchReply)}`);
            assert.equal(survivor["workspace"], "ws-2", "B survivor stays tiled on B");
            const survivorRect = survivor["rect"] as { x: number; y: number; w: number; h: number };
            assert.deepEqual(
                winC["frameGeometry"],
                { x: survivorRect["x"], y: survivorRect["y"], width: survivorRect["w"], height: survivorRect["h"] },
                "B survivor converges to its reflowed tile instead of being displaced",
            );
            assert.deepEqual(winC["desktops"], [d2], "B survivor stays homed on B");
            const aTile = (floatReply["desired_geometry"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-b");
            assert.ok(aTile !== undefined, "setup: sticky-on reflows the A survivor");
            const aRect = aTile["rect"] as { x: number; y: number; w: number; h: number };
            assert.deepEqual(
                winB["frameGeometry"],
                { x: aRect["x"], y: aRect["y"], width: aRect["w"], height: aRect["h"] },
                "no source A topology relocation past the legitimate sticky-on reflow",
            );
            assert.deepEqual(winB["desktops"], [world.desktop], "A survivor stays homed on A");
            const desired = switchReply["desired_geometry"] as Array<Record<string, unknown>>;
            assert.ok(!desired.some((entry) => entry["window"] === "win-b"), "A survivor absent from the B tiling");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("rejected switched unfloat strands nothing: plain Meta+G then tiles B", async () => {
        // The first switched unfloat reply is rejected before apply while
        // native sticky-off already succeeded; a plain Meta+G must still
        // tile B instead of stranding the window floating.
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            const d2 = { id: "ws-2" };
            const { switchCorrelation } = await driveStickyToSwitchedWorkspace(world, mocks, resolved, engine, d2, 0, false);
            mocks.callbacks[1]?.(rejectedReply(switchCorrelation, "not-tiled"));
            assert.ok(
                mocks.logs.some((line) => line.includes("outcome=rejected")),
                `rejection settles the flight, logs: ${tailLogs(mocks)}`,
            );
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 3, `stranded float retries tiling, logs: ${tailLogs(mocks)}`);
            const retryCmd = (JSON.parse(mocks.dbusCalls[2]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>;
            assert.ok("float_rect" in retryCmd, `retry still carries the retained float state, got ${JSON.stringify(retryCmd)}`);
            const retryReply = await flushPlan(mocks, engine, 2);
            assert.equal(
                retryReply["outcome"],
                "planned",
                `plain Meta+G tiles B after rejection, engine replied ${JSON.stringify(retryReply)}, logs: ${tailLogs(mocks)}`,
            );
            const winA = world.wins[0] as Record<string, unknown>;
            assertTiledOnWorkspace(world, mocks, winA, d2, retryReply, "post-rejection return");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    // Foreground automatic reconcile through the production observer and
    // the real Engine: every dispatch below carries op reconcile with the
    // complete observation, never a baseline-derived admit/remove.
    function fireAdded(world: FakeWorld, mocks: EntryMocks): void {
        for (const handler of [...world.added.handlers]) {
            handler();
        }
        runEntryDebounce(mocks);
    }

    function reconcileOp(mocks: EntryMocks, index: number): Record<string, unknown> {
        const payload = JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
        assert.deepEqual(
            (payload["command"] as Record<string, unknown>)["op"],
            "reconcile",
            `dispatch ${index} carries op reconcile, logs: ${tailLogs(mocks)}`,
        );
        return payload;
    }

    it("foreground automatic reconcile seeds a fresh fit through the real Engine", async () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            fireAdded(world, mocks);
            assert.equal(mocks.dbusCalls.length, 1, `fresh signal dispatches once, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 0);
            const reply = await flushPlan(mocks, engine, 0);
            assert.equal(reply["outcome"], "planned", `fresh fit plans, engine replied ${JSON.stringify(reply)}`);
            const desired = reply["desired_geometry"] as Array<Record<string, unknown>>;
            const windows = desired.map((entry) => entry["window"]).sort();
            assert.deepEqual(windows, ["win-a", "win-b"]);
            const rectA = (desired.find((entry) => entry["window"] === "win-a") as Record<string, unknown>)["rect"] as {
                x: number; y: number; w: number; h: number;
            };
            assert.deepEqual(
                (world.wins[0] as Record<string, unknown>)["frameGeometry"],
                { x: rectA["x"], y: rectA["y"], width: rectA["w"], height: rectA["h"] },
                "fresh tile applied natively",
            );
            // A settled observation dispatches nothing further.
            const calls = mocks.dbusCalls.length;
            fireAdded(world, mocks);
            assert.equal(mocks.dbusCalls.length, calls, "converged observation stays quiet");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("foreground automatic reconcile converges membership transitions through the real Engine", async () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            fireAdded(world, mocks);
            const seed = await flushPlan(mocks, engine, 0);
            assert.equal(seed["outcome"], "planned", `setup seed plans, engine replied ${JSON.stringify(seed)}`);
            // Newcomer win-c joins through the same automatic route.
            addTiledWindow(world, "win-c", world.desktop, 400);
            fireAdded(world, mocks);
            assert.equal(mocks.dbusCalls.length, 2, `newcomer dispatches once, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 1);
            const grown = await flushPlan(mocks, engine, 1);
            assert.equal(grown["outcome"], "planned", `newcomer converges, engine replied ${JSON.stringify(grown)}`);
            const grownWindows = ((grown["desired_geometry"] as Array<Record<string, unknown>>).map((entry) => entry["window"]) as string[]).sort();
            assert.deepEqual(grownWindows, ["win-a", "win-b", "win-c"]);
            // Departure converges back to the survivors.
            const [departed] = world.wins.splice(2, 1);
            for (const handler of [...world.removed.handlers]) {
                (handler as unknown as (removed?: unknown) => void)(departed);
            }
            const survivor = world.wins[0] as Record<string, unknown>;
            fireGeometry(world, survivor);
            runEntryDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 3, `departure dispatches once, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 2);
            const shrunk = await flushPlan(mocks, engine, 2);
            assert.equal(shrunk["outcome"], "planned", `departure converges, engine replied ${JSON.stringify(shrunk)}`);
            const shrunkWindows = ((shrunk["desired_geometry"] as Array<Record<string, unknown>>).map((entry) => entry["window"]) as string[]).sort();
            assert.deepEqual(shrunkWindows, ["win-a", "win-b"]);
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("foreground gap-mismatch retry lands one update-gaps through the real Engine", async () => {
        const world = fakeWorld();
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            fireAdded(world, mocks);
            const seed = await flushPlan(mocks, engine, 0);
            assert.equal(seed["outcome"], "planned", `setup seed plans, engine replied ${JSON.stringify(seed)}`);
            // Drift the native frame so the next signal dispatches a
            // reconcile, then refuse it with the exact Engine inner-gap
            // message: the adapter must retry once with update-gaps.
            const winA = world.wins[0] as Record<string, unknown>;
            winA["frameGeometry"] = { x: 0, y: 0, width: 750, height: 800 };
            fireGeometry(world, winA);
            runEntryDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, `drift dispatches, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 1);
            const correlation = (JSON.parse(mocks.dbusCalls[1]?.payload as string) as Record<string, unknown>)["correlation_id"] as string;
            mocks.callbacks[1]?.(
                JSON.stringify({
                    v: 1,
                    correlation_id: correlation,
                    outcome: "rejected",
                    kind: "domain-mismatch",
                    message: "domain gap does not match retained state",
                }),
            );
            assert.equal(mocks.dbusCalls.length, 3, `exactly one update-gaps retry, logs: ${tailLogs(mocks)}`);
            const retry = JSON.parse(mocks.dbusCalls[2]?.payload as string) as Record<string, unknown>;
            assert.deepEqual((retry["command"] as Record<string, unknown>)["op"], "update-gaps");
            const retried = await flushPlan(mocks, engine, 2);
            assert.equal(retried["outcome"], "planned", `update-gaps retry plans, engine replied ${JSON.stringify(retried)}`);
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("sticky-on then automatic flag reconcile keeps the exact subject active", async () => {
        // Trace plan-1 p5/p6 against the real Engine: synchronous sticky echo
        // inside the native write, then the automatic flag-change reconcile
        // must not activate the tiled survivor.
        const world = fakeWorld();
        const winA = world.wins[0] as Record<string, unknown>;
        const winB = world.wins[1] as Record<string, unknown>;
        let stickyFlag = false;
        Object.defineProperty(winA, "onAllDesktops", {
            get: (): boolean => stickyFlag,
            set: (value: unknown): void => {
                stickyFlag = value === true;
                for (const handler of [...(world.winDesktops.get(winA)?.handlers ?? [])]) {
                    handler();
                }
            },
            enumerable: true,
            configurable: true,
        });
        const activeWrites: object[] = [];
        let activeCurrent: object = winA;
        Object.defineProperty(world.workspace, "activeWindow", {
            get: (): object => activeCurrent,
            set: (value: object): void => {
                activeWrites.push(value);
                activeCurrent = value;
            },
            enumerable: true,
            configurable: true,
        });
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            assert.equal(world.workspace["activeWindow"], winA, "setup: subject starts active");
            fireAdded(world, mocks);
            assert.equal(mocks.dbusCalls.length, 1, `startup dispatches once, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 0);
            const seed = await flushPlan(mocks, engine, 0);
            assert.equal(seed["outcome"], "planned", `setup: startup fits, engine replied ${JSON.stringify(seed)}`);
            assert.equal(world.workspace["activeWindow"], winA, "setup: startup keeps the subject active");
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 2, `float dispatches, logs: ${tailLogs(mocks)}`);
            const floated = await flushPlan(mocks, engine, 1);
            assert.equal(floated["outcome"], "planned", `setup: float plans, engine replied ${JSON.stringify(floated)}`);
            assert.equal(world.workspace["activeWindow"], winA, "setup: plain float retains the subject");
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 3, `unfloat dispatches, logs: ${tailLogs(mocks)}`);
            const tiled = await flushPlan(mocks, engine, 2);
            assert.equal(tiled["outcome"], "planned", `setup: unfloat plans, engine replied ${JSON.stringify(tiled)}`);
            assert.equal(world.workspace["activeWindow"], winA, "setup: unfloat retains the subject");
            const base = mocks.logs.length;
            resolved.requestSticky();
            assert.equal(mocks.dbusCalls.length, 4, `sticky-on dispatches, logs: ${tailLogs(mocks)}`);
            const stickyReply = await flushPlan(mocks, engine, 3);
            assert.equal(stickyReply["outcome"], "planned", `setup: sticky-on plans, engine replied ${JSON.stringify(stickyReply)}`);
            assert.equal(winA["onAllDesktops"], true, "setup: sticky-on takes native all-desktops");
            const trail = mocks.logs.slice(base);
            const order = (token: string): number => trail.findIndex((line) => line.includes(token));
            const issued = order("target=all-desktops outcome=issued");
            const consumed = order("sticky-echo-consumed");
            const invoked = order("target=all-desktops outcome=invoked");
            const retained = order("sticky-focus-retained");
            const applied = order("outcome=planned-applied");
            assert.ok(issued >= 0 && consumed > issued && invoked > consumed && retained > invoked && applied > retained, `native order issued<consumed<invoked<retained<applied, logs: ${trail.join(" | ")}`);
            assert.equal(world.workspace["activeWindow"], winA, "setup: sticky-on retains the subject");
            fireGeometry(world, winA);
            runEntryDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 5, `flag change dispatches one auto reconcile, logs: ${tailLogs(mocks)}`);
            const autoPayload = reconcileOp(mocks, 4);
            assert.equal(autoPayload["focused_window"], "win-a", "auto reconcile request still names the subject");
            const autoReply = await flushPlan(mocks, engine, 4);
            assert.equal(autoReply["outcome"], "planned", `auto reconcile plans, engine replied ${JSON.stringify(autoReply)}`);
            const focus = autoReply["desired_focus"] as Record<string, unknown> | undefined;
            const geometry = autoReply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
            const focusWindow = Array.isArray(geometry)
                ? geometry.find((entry) => entry["leaf"] === focus?.["leaf"])?.["window"]
                : undefined;
            assert.equal(focusWindow, "win-b", "planner names the tiled survivor while native focus stays on the subject");
            assert.ok(mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("kind=reconcile") && line.includes("reason=floating-active")), "skip gate engages instead of actuating survivor focus");
            assert.equal(world.workspace["activeWindow"], winA, "auto reconcile keeps the subject active");
            assert.ok(!activeWrites.includes(winB), "zero native writes activating the sibling across the automatic reconcile");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("half-snap on an ordinary float survives the automatic post-snap reconcile", async () => {
        // Causal follow-up for the float-origin half-snap: the synchronous
        // snap writes native geometry, the real frameGeometryChanged signal
        // plus debounce dispatches one automatic reconcile, and the Engine
        // reply must not rewrite the snapped float, admit it to tiling, or
        // steal its focus. Assertions land after the reconcile callback.
        const world = fakeWorld();
        const winA = world.wins[0] as Record<string, unknown>;
        const winB = world.wins[1] as Record<string, unknown>;
        const activeWrites: object[] = [];
        let activeCurrent: object = winA;
        Object.defineProperty(world.workspace, "activeWindow", {
            get: (): object => activeCurrent,
            set: (value: object): void => {
                activeWrites.push(value);
                activeCurrent = value;
            },
            enumerable: true,
            configurable: true,
        });
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            fireAdded(world, mocks);
            assert.equal(mocks.dbusCalls.length, 1, `seed dispatches once, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, 0);
            const seed = await flushPlan(mocks, engine, 0);
            assert.equal(seed["outcome"], "planned", `setup: seed plans, engine replied ${JSON.stringify(seed)}`);
            resolved.requestFloat();
            assert.equal(mocks.dbusCalls.length, 2, `float dispatches, logs: ${tailLogs(mocks)}`);
            const floated = await flushPlan(mocks, engine, 1);
            assert.equal(floated["outcome"], "planned", `setup: float plans, engine replied ${JSON.stringify(floated)}`);
            assert.equal(world.workspace["activeWindow"], winA, "setup: float keeps the subject active");
            const callsBeforeSnap = mocks.dbusCalls.length;
            resolved.requestMove("left");
            assert.ok(mocks.logs.some((line) => line.includes("move-float-applied direction=left")), `snap applies, logs: ${tailLogs(mocks)}`);
            assert.equal(mocks.dbusCalls.length, callsBeforeSnap, "snap dispatches nothing to the Engine");
            const snapped = { ...(winA["frameGeometry"] as Record<string, unknown>) };
            assert.deepEqual(snapped, { x: 8, y: 8, width: 588, height: 784 }, "snap writes the COSMIC left half");
            const writesBeforeReconcile = activeWrites.length;
            fireGeometry(world, winA);
            runEntryDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsBeforeSnap + 1, `post-snap geometry dispatches one auto reconcile, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, callsBeforeSnap);
            const reply = await flushPlan(mocks, engine, callsBeforeSnap);
            assert.equal(reply["outcome"], "planned", `auto reconcile plans, engine replied ${JSON.stringify(reply)}`);
            const desired = reply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
            assert.ok(Array.isArray(desired) && !desired.some((entry) => entry["window"] === "win-a"), `no tile admission for the snapped float, got ${JSON.stringify(reply)}`);
            assert.deepEqual(winA["frameGeometry"], snapped, "reconcile rewrites nothing on the snapped float");
            assert.equal(world.workspace["activeWindow"], winA, "reconcile keeps the snapped float focused");
            assert.ok(!activeWrites.slice(writesBeforeReconcile).includes(winB), "zero sibling activation across the post-snap reconcile");
            assert.ok(mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("kind=reconcile")), "skip gate holds survivor focus instead of actuating it");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

    it("half-snap on a sticky float survives the automatic post-snap reconcile", async () => {
        // Sticky counterpart: sticky-on from the floated origin, then the
        // same snap plus signal plus debounce plus reconcile path. The
        // snapped sticky keeps native geometry and focus with no admission.
        const world = fakeWorld();
        const winA = world.wins[0] as Record<string, unknown>;
        const winB = world.wins[1] as Record<string, unknown>;
        const activeWrites: object[] = [];
        let activeCurrent: object = winA;
        Object.defineProperty(world.workspace, "activeWindow", {
            get: (): object => activeCurrent,
            set: (value: object): void => {
                activeWrites.push(value);
                activeCurrent = value;
            },
            enumerable: true,
            configurable: true,
        });
        const { handle, mocks } = startEntry(world);
        assert.ok(handle !== null, "entry enabled");
        const engine = StickyEngineBridge.start();
        try {
            const resolved = handle as StickyHandle;
            fireAdded(world, mocks);
            reconcileOp(mocks, 0);
            const seed = await flushPlan(mocks, engine, 0);
            assert.equal(seed["outcome"], "planned", `setup: seed plans, engine replied ${JSON.stringify(seed)}`);
            resolved.requestFloat();
            const floated = await flushPlan(mocks, engine, 1);
            assert.equal(floated["outcome"], "planned", `setup: float plans, engine replied ${JSON.stringify(floated)}`);
            resolved.requestSticky();
            assert.equal(winA["onAllDesktops"], true, "setup: sticky-on takes native all-desktops");
            assert.equal(world.workspace["activeWindow"], winA, "setup: sticky-on retains the subject");
            const callsBeforeSnap = mocks.dbusCalls.length;
            resolved.requestMove("right");
            assert.ok(mocks.logs.some((line) => line.includes("move-float-applied direction=right")), `snap applies, logs: ${tailLogs(mocks)}`);
            assert.equal(mocks.dbusCalls.length, callsBeforeSnap, "snap dispatches nothing to the Engine");
            const snapped = { ...(winA["frameGeometry"] as Record<string, unknown>) };
            assert.deepEqual(snapped, { x: 604, y: 8, width: 588, height: 784 }, "snap writes the COSMIC right half");
            assert.equal(winA["onAllDesktops"], true, "snap keeps native stickiness");
            const writesBeforeReconcile = activeWrites.length;
            fireGeometry(world, winA);
            runEntryDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsBeforeSnap + 1, `post-snap geometry dispatches one auto reconcile, logs: ${tailLogs(mocks)}`);
            reconcileOp(mocks, callsBeforeSnap);
            const reply = await flushPlan(mocks, engine, callsBeforeSnap);
            assert.equal(reply["outcome"], "planned", `auto reconcile plans, engine replied ${JSON.stringify(reply)}`);
            const desired = reply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
            assert.ok(Array.isArray(desired) && !desired.some((entry) => entry["window"] === "win-a"), `no tile admission for the snapped sticky, got ${JSON.stringify(reply)}`);
            assert.deepEqual(winA["frameGeometry"], snapped, "reconcile rewrites nothing on the snapped sticky");
            assert.equal(winA["onAllDesktops"], true, "reconcile keeps native stickiness");
            assert.equal(world.workspace["activeWindow"], winA, "reconcile keeps the snapped sticky focused");
            assert.ok(!activeWrites.slice(writesBeforeReconcile).includes(winB), "zero sibling activation across the post-snap reconcile");
            assert.ok(mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("kind=reconcile")), "skip gate holds survivor focus instead of actuating it");
        } finally {
            handle?.stop();
            await engine.close();
        }
    });

});

describe("plan sequence rotation", () => {
    it("rotates the correlation namespace after 1M plans without permanent refusal", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const sessionBefore = (adapter as unknown as { plannerSession: number }).plannerSession;
        (adapter as unknown as { seq: number }).seq = 1000000;
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        const first = plannerPayload(mocks, 0)["correlation_id"] as string;
        assert.equal(first, "gen-1-p1000000");
        mocks.callbacks[0]?.(rejectedReply(first, "snapshot-invalid"));
        assert.equal(adapter.isInFlight, false);
        adapter.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 2);
        const second = plannerPayload(mocks, 1)["correlation_id"] as string;
        assert.equal(second, "gen-1-p1r0");
        assert.notEqual(second, first);
        assert.ok(second.length > 0 && second.length <= 128);
        mocks.callbacks[0]?.(rejectedReply(first, "snapshot-invalid"));
        assert.equal(adapter.isInFlight, true, "a delayed old-epoch reply cannot settle the new flight");
        const line = mocks.logs.find((l) => l.includes("sequence-rotated") && l.includes(`correlation=${second}`));
        assert.ok(line !== undefined, mocks.logs.join("\n"));
        assert.ok(line.includes("cause=sequence-exhausted") && line.includes("recovery=rotated"), line);
        for (const raw of ["win-a", "win-b", "ws-1", "ws-2", "out-1", ":1.7", "owner-1"]) {
            assert.ok(!line.includes(raw), `${raw} leaked in:\n${line}`);
        }
        assert.equal(adapter.isEnabled, true);
        assert.equal((adapter as unknown as { plannerSession: number }).plannerSession, sessionBefore);
    });
});

describe("plan adapter refresh classification", () => {
    const allocA = { x: 0, y: 0, w: 600, h: 800 };
    const allocB = { x: 600, y: 0, w: 600, h: 800 };
    function baselineRefresh(mocks: Mocks, refs: { a: object; b: object; c: object }): PlanAdapter {
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": allocA, "win-b": allocB } });
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runTimers(mocks);
        const corr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(plannedReply(corr, [{ window: "win-a", rect: allocA }, { window: "win-b", rect: allocB }], "win-a-leaf"));
        return adapter;
    }
    function refreshLines(mocks: Mocks): string[] {
        return mocks.logs.filter((line) =>
            line.startsWith("omnitiler:route-diag component=cosmic-plan route=plan stage=refresh event=foreground "),
        );
    }
    it("quiet equal emits one bounded equal classification with no dispatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineRefresh(mocks, refs);
        const callsBefore = mocks.dbusCalls.length;
        const logsBefore = refreshLines(mocks).length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore, "equal observation stays quiet");
        const fresh = refreshLines(mocks).slice(logsBefore);
        assert.equal(fresh.length, 1, `one classification per decision, got ${JSON.stringify(fresh)}`);
        assert.ok(
            fresh[0] === `omnitiler:route-diag component=cosmic-plan route=plan stage=refresh event=foreground outcome=equal reason=applied-evidence-equal terminal=quiet correlation=none generation=gen-1`,
            fresh[0] as string,
        );
        for (const raw of ["win-a", "win-b", "600,800", "0,0"]) {
            assert.ok(!(fresh[0] as string).includes(raw), `${raw} leaked in:\n${fresh[0]}`);
        }
    });
    it("membership drift emits change correlated with the generated dispatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineRefresh(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, rects: { "win-a": { x: 0, y: 0, w: 616, h: 800 }, "win-b": allocB } });
        const logsBefore = refreshLines(mocks).length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        const corr = plannerPayload(mocks, mocks.dbusCalls.length - 1)["correlation_id"] as string;
        const fresh = refreshLines(mocks).slice(logsBefore);
        assert.equal(fresh.length, 1);
        assert.ok(
            (fresh[0] as string).includes("outcome=change") &&
                (fresh[0] as string).includes("reason=drift") &&
                (fresh[0] as string).includes("terminal=dispatch") &&
                (fresh[0] as string).includes(`correlation=${corr}`),
            fresh[0] as string,
        );
        assert.ok(!(fresh[0] as string).includes("win-a") && !(fresh[0] as string).includes("616,800"), fresh[0] as string);
    });
    it("work-area transition still reprojects when flags differ", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineRefresh(mocks, refs);
        const scaled = { x: 0, y: 0, w: 1800, h: 1200 };
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, bounds: scaled, rects: { "win-a": allocA, "win-b": allocB }, floating: { "win-a": true } });
        const logsBefore = refreshLines(mocks).length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:scope-transition old=0,0,1200,800 new=0,0,1800,1200"),
            "flag-differing transition still takes the window-set reprojection path",
        );
        const fresh = refreshLines(mocks).slice(logsBefore);
        assert.equal(fresh.length, 1);
        assert.ok(
            (fresh[0] as string).includes("outcome=change") &&
                (fresh[0] as string).includes("reason=work-area-transition") &&
                (fresh[0] as string).includes("terminal=dispatch"),
            fresh[0] as string,
        );
    });
    it("raw retained out-of-bounds bypasses quiet with a change dispatch", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineRefresh(mocks, refs);
        const callsBefore = mocks.dbusCalls.length;
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, rects: { "win-a": { x: 2000, y: 0, w: 600, h: 800 }, "win-b": allocB } });
        const logsBefore = refreshLines(mocks).length;
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, callsBefore + 1, "raw out-of-bounds must reconcile");
        const fresh = refreshLines(mocks).slice(logsBefore);
        assert.equal(fresh.length, 1);
        assert.ok(
            (fresh[0] as string).includes("outcome=change") &&
                (fresh[0] as string).includes("reason=raw-out-of-bounds") &&
                (fresh[0] as string).includes("terminal=dispatch"),
            fresh[0] as string,
        );
    });
});

describe("D01 native activation ownership focus fence", () => {
    const RA = { x: 0, y: 0, w: 600, h: 800 };
    const RB = { x: 600, y: 0, w: 600, h: 800 };
    const equalGeom = [
        { window: "win-a", rect: RA },
        { window: "win-b", rect: RB },
    ];

    function plainObserved(refs: { a: object; b: object; c: object }, focused: object): PlanObserved {
        return makeObserved(refs, { focused, rects: { "win-a": RA, "win-b": RB } });
    }

    // Two retained tiles focused on A. Returns the enabled adapter with
    // applied evidence for both members, so a later fullscreen on the same
    // ref keeps its tile slot instead of taking the born-fullscreen hold.
    function baselineTwoTiles(mocks: Mocks, refs: { a: object; b: object; c: object }): PlanAdapter {
        mocks.observeImpl = () => plainObserved(refs, refs.a);
        mocks.activeImpl = () => refs.a;
        const adapter = enableAdapter(mocks);
        fire(mocks, "added");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "baseline reconcile must dispatch");
        const corr = plannerPayload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(plannedReply(corr, equalGeom, "win-a-leaf"));
        assert.deepEqual(mocks.actives, [], "baseline settles with no focus write");
        return adapter;
    }

    function dispatchReconcile(mocks: Mocks, index: number): string {
        // A flag/focus drift (not a bare re-added signal, which stays
        // quiet on equal membership) drives the automatic reconcile.
        fire(mocks, "geometry");
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, index + 1, "automatic reconcile must dispatch");
        return plannerPayload(mocks, index)["correlation_id"] as string;
    }

    it("fences an automatic focus write while a retained fullscreen window is present", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // Host kept the fullscreen game B focused; the tiler must not yank
        // focus back to A on a divergent automatic reply. The sibling drift
        // on A drives the reconcile dispatch (pure flag flips stay quiet).
        const driftA = { x: 0, y: 0, w: 620, h: 800 };
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.b,
                rects: { "win-a": driftA, "win-b": RB },
                fullscreen: { "win-b": true },
            });
        mocks.activeImpl = () => refs.b;
        const corr = dispatchReconcile(mocks, 1);
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: driftA },
                    { window: "win-b", rect: RB },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.actives, [], "no focus steal while fullscreen involved");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=fullscreen-active")),
            mocks.logs.join("\n"),
        );
    });

    it("fences an automatic focus write while a maximized overlay is present", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.b, rects: { "win-a": RA, "win-b": RB }, maximized: { "win-b": true } });
        mocks.activeImpl = () => refs.b;
        enableAdapter(mocks);
        const corr = dispatchReconcile(mocks, 0);
        mocks.callbacks[0]?.(plannedReply(corr, equalGeom, "win-a-leaf"));
        assert.deepEqual(mocks.actives, [], "no focus steal while maximized involved");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=maximize-active")),
            mocks.logs.join("\n"),
        );
    });

    it("fences a mid-apply native focus race to the retained fullscreen window", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // Dispatch snapshot reads native on A; native switches to retained
        // fullscreen B after the observation, so the reply validates while
        // the just-before-focus re-read carries the overlay. No new setters
        // are used to detect it. The sibling drift on A drives dispatch.
        const driftA = { x: 0, y: 0, w: 620, h: 800 };
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": driftA, "win-b": RB },
                fullscreen: { "win-b": true },
            });
        mocks.activeImpl = () => refs.b;
        const corr = dispatchReconcile(mocks, 1);
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: driftA },
                    { window: "win-b", rect: RB },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.actives, [], "live native focus on fullscreen B must stand");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=fullscreen-active")),
            mocks.logs.join("\n"),
        );
    });

    it("fences fullscreen onset triggered from inside a geometry write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // True onset (not preset): no fullscreen anywhere at dispatch or
        // at the reply boundary. The sibling enters fullscreen natively
        // from inside the first geometry setter, and native focus follows
        // it before the focus setter runs. The just-before-focus re-read
        // is the only evidence; the reply-boundary observation is clean.
        // The sibling drift on A drives dispatch and the first write.
        let fsOn = false;
        let liveActive: object = refs.a;
        const driftA = { x: 0, y: 0, w: 620, h: 800 };
        const writeA = { x: 0, y: 0, w: 640, h: 800 };
        mocks.observeImpl = () =>
            makeObserved(refs, {
                focused: refs.a,
                rects: { "win-a": driftA, "win-b": RB },
                ...(fsOn ? { fullscreen: { "win-b": true } } : {}),
            });
        mocks.activeImpl = () => liveActive;
        const corr = dispatchReconcile(mocks, 1);
        const origGeometry = mocks.geometryImpl;
        mocks.geometryImpl = (target, rect) => {
            const ok = origGeometry(target, rect);
            fsOn = true;
            liveActive = refs.b;
            return ok;
        };
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: writeA },
                    { window: "win-b", rect: RB },
                ],
                "win-a-leaf",
            ),
        );
        assert.equal(mocks.geometries.length, 1, "the drifted write still lands");
        assert.deepEqual(mocks.actives, [], "onset fullscreen must fence the trailing focus write");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=fullscreen-active")),
            mocks.logs.join("\n"),
        );
    });

    it("suppresses automatic focus when the foreground domain changes mid-apply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // Native switches the active output mid-apply: the foreground
        // re-read names another domain, so the stale reply focus (bound to
        // the old domain) must not actuate there.
        let domainNow = { output: "out-1", workspace: "ws-1" };
        const driftA = { x: 0, y: 0, w: 620, h: 800 };
        const writeA = { x: 0, y: 0, w: 640, h: 800 };
        mocks.observeImpl = () => ({
            ...makeObserved(refs, { focused: refs.a, rects: { "win-a": driftA, "win-b": RB } }),
            domainOutput: domainNow.output,
            domainWorkspace: domainNow.workspace,
        });
        mocks.activeImpl = () => refs.a;
        const corr = dispatchReconcile(mocks, 1);
        const origGeometry = mocks.geometryImpl;
        mocks.geometryImpl = (target, rect) => {
            const ok = origGeometry(target, rect);
            domainNow = { output: "out-2", workspace: "ws-1" };
            return ok;
        };
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: writeA },
                    { window: "win-b", rect: RB },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.actives, [], "domain-changed automatic focus must not actuate");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=domain-changed")),
            mocks.logs.join("\n"),
        );
    });

    it("suppresses automatic focus when live active moves after the re-read", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // No overlay anywhere: the re-read matches the reply boundary, but
        // the live active() no longer matches the re-read activeRef, so the
        // observation is already stale and the automatic write is dropped.
        const driftA = { x: 0, y: 0, w: 620, h: 800 };
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, rects: { "win-a": driftA, "win-b": RB } });
        mocks.activeImpl = () => refs.b;
        const corr = dispatchReconcile(mocks, 1);
        mocks.callbacks[1]?.(
            plannedReply(
                corr,
                [
                    { window: "win-a", rect: driftA },
                    { window: "win-b", rect: RB },
                ],
                "win-a-leaf",
            ),
        );
        assert.deepEqual(mocks.actives, [], "active-moved automatic focus must not actuate");
        assert.ok(
            mocks.logs.some((line) => line.includes("focus-skipped") && line.includes("reason=active-changed")),
            mocks.logs.join("\n"),
        );
    });

    it("explicit directional focus still actuates with a retained fullscreen window present", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = baselineTwoTiles(mocks, refs);
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, rects: { "win-a": RA, "win-b": RB }, fullscreen: { "win-b": true } });
        mocks.activeImpl = () => refs.a;
        adapter.requestFocus("right");
        assert.equal(mocks.dbusCalls.length, 2, "explicit focus must still dispatch");
        const corr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(plannedReply(corr, equalGeom, "win-b-leaf"));
        assert.deepEqual(mocks.actives, [refs.b], "explicit op focus stays effective");
        assert.ok(
            !mocks.logs.some((line) => line.includes("focus-skipped")),
            "explicit focus must never hit the automatic fence",
        );
    });

    it("explicit directional focus exits fullscreen to the sibling", () => {
        const refs2 = makeRefs();
        const mocks2 = mockEnv(refs2);
        const adapter2 = baselineTwoTiles(mocks2, refs2);
        // Fullscreen game B focused; the user explicitly navigates left to
        // sibling A. The reply completes and the explicit write lands.
        mocks2.observeImpl = () =>
            makeObserved(refs2, {
                focused: refs2.b,
                rects: { "win-a": RA, "win-b": RB },
                fullscreen: { "win-b": true },
            });
        mocks2.activeImpl = () => refs2.b;
        adapter2.requestFocus("left");
        assert.equal(mocks2.dbusCalls.length, 2, "explicit navigation must still dispatch");
        const corr = plannerPayload(mocks2, 1)["correlation_id"] as string;
        mocks2.callbacks[1]?.(plannedReply(corr, equalGeom, "win-a-leaf"));
        assert.deepEqual(mocks2.actives, [refs2.a], "explicit fullscreen exit focuses the sibling");
        assert.ok(
            !mocks2.logs.some((line) => line.includes("focus-skipped")),
            "explicit focus must never hit the automatic fence",
        );
    });

    function singleObserved(refs: { a: object; b: object; c: object }): PlanObserved {
        return {
            domainOutput: "out-1",
            domainWorkspace: "ws-1",
            domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
            domainGap: DOMAIN_GAP,
            domainOuterGap: OUTER_DOMAIN_GAP,
            focusedId: "win-a",
            windows: Object.freeze([
                Object.freeze({
                    id: "win-a",
                    ref: refs.a,
                    rect: RA,
                    output: "out-1",
                    workspace: "ws-1",
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
            ]),
            activeRef: refs.a,
            fingerprint: "fp-close",
            revalidate: () => true,
        };
    }

    it("survivor refocus actuates after the fullscreen overlay closes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        baselineTwoTiles(mocks, refs);
        // Fullscreen B closes: the removal observation carries only
        // survivor A, so the refill reconcile has no overlay left and the
        // survivor refocus actuates even with stale native focus on the
        // removed window.
        mocks.observeImpl = () => singleObserved(refs);
        mocks.activeImpl = () => refs.b;
        fire(mocks, "removed", refs.b);
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 2, "removal must reconcile the refill");
        const corr = plannerPayload(mocks, 1)["correlation_id"] as string;
        mocks.callbacks[1]?.(plannedReply(corr, [{ window: "win-a", rect: RA }], "win-a-leaf"));
        assert.deepEqual(mocks.actives, [refs.a], "close refill focuses the survivor");
        assert.ok(
            !mocks.logs.some((line) => line.includes("focus-skipped")),
            "overlay-free refill must never hit the fence",
        );
    });

    it("close-race staleness after the overlay leaves writes no focus", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // Dispatch while fullscreen B is still around; B closes before the
        // reply lands, so the reply boundary goes stale with zero setters.
        mocks.observeImpl = () =>
            makeObserved(refs, { focused: refs.a, rects: { "win-a": RA, "win-b": RB }, fullscreen: { "win-b": true } });
        mocks.activeImpl = () => refs.a;
        enableAdapter(mocks);
        const corr = dispatchReconcile(mocks, 0);
        mocks.observeImpl = () => makeObserved(refs, { focused: refs.a, rects: { "win-a": RA, "win-b": RB } });
        mocks.callbacks[0]?.(plannedReply(corr, equalGeom, "win-a-leaf"));
        assert.deepEqual(mocks.actives, [], "stale close-race reply must not actuate");
        assert.deepEqual(mocks.geometries, [], "stale close-race reply must not write geometry");
    });
});
