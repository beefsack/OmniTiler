import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    PlanAdapter,
    type PlanAdapterEnv,
    type PlanObserved,
    type PlanObservedWindow,
} from "../src/plan-adapter";
import { DOMAIN_GAP, OUTER_DOMAIN_GAP } from "../src/domain-gap";

// R-OUT-05 PlanAdapter-level integration: actual signal -> debounced
// foreground refresh -> route -> admission against a two-output native
// model. Covers both transfer directions, exact native call order,
// adoption / view-switch / user-movement no-route, transient / game /
// fullscreen / overlay safeguards, hidden targets, and failed writes.

interface TWin {
    readonly ref: object;
    readonly id: string;
    output: string;
    desks: string[];
    rect: { x: number; y: number; w: number; h: number };
    fullscreen: boolean;
    maximized: boolean;
    floating: boolean;
    sticky: boolean;
    transient: boolean | undefined;
    fixedMin?: { w: number; h: number };
    fixedMax?: { w: number; h: number };
}

interface TWorld {
    outputs: Map<string, { cur: string; obj: object }>;
    desks: Map<string, object>;
    wins: TWin[];
    active: object | null;
    floatIds: Set<string>;
}

const OUT_F = "out-f";
const OUT_P = "out-p";
const WS_F = "ws-f";
const WS_P = "ws-p";

function makeWorld(): TWorld {
    return {
        outputs: new Map([
            [OUT_F, { cur: WS_F, obj: {} }],
            [OUT_P, { cur: WS_P, obj: {} }],
        ]),
        desks: new Map([
            [WS_F, {}],
            ["ws-other", {}],
            [WS_P, {}],
        ]),
        wins: [],
        active: null,
        floatIds: new Set<string>(),
    };
}

function addWin(world: TWorld, id: string, output: string, ws: string): TWin {
    const win: TWin = {
        ref: {}, id, output, desks: [ws],
        rect: { x: 0, y: 0, w: 100, h: 80 },
        fullscreen: false, maximized: false, floating: false, sticky: false, transient: false,
    };
    world.wins.push(win);
    return win;
}

interface Harness {
    env: PlanAdapterEnv;
    order: string[];
    payloads: Array<Record<string, unknown>>;
    timers: Array<{ callback: () => void; cancelled: boolean }>;
    subs: Map<string, (target?: object) => void>;
    logs: string[];
    world: TWorld;
    observes: number;
    byId: (id: string) => TWin;
}

function observeWorld(world: TWorld, harness: { observes: number }): PlanObserved | null {
    harness.observes += 1;
    const activeWin = world.wins.find((w) => w.ref === world.active) ?? null;
    if (activeWin === null) {
        return null;
    }
    const output = activeWin.output;
    const shown = world.outputs.get(output)?.cur;
    if (shown === undefined) {
        return null;
    }
    const windows: PlanObservedWindow[] = [];
    for (const w of world.wins) {
        if (w.output !== output || !w.desks.includes(shown)) {
            continue;
        }
        windows.push({
            id: w.id, ref: w.ref,
            rect: { x: w.rect.x, y: w.rect.y, w: w.rect.w, h: w.rect.h },
            output, workspace: shown,
            fullscreen: w.fullscreen, maximized: w.maximized,
            ...(w.floating || world.floatIds.has(w.id) ? { floating: true as const } : {}),
            ...(w.sticky ? { sticky: true as const } : {}),
        });
    }
    const member = windows.some((w) => w.ref === world.active);
    return {
        domainOutput: output,
        domainWorkspace: shown,
        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: DOMAIN_GAP,
        domainOuterGap: OUTER_DOMAIN_GAP,
        focusedId: member ? (activeWin.id) : "",
        windows: Object.freeze(windows),
        activeRef: world.active as object,
        fingerprint: `fp-${output}-${shown}-${windows.map((w) => w.id).sort().join(",")}`,
        revalidate: () => true,
    };
}

function plannedEcho(correlation: string, payload: Record<string, unknown>): string {
    const windows = (payload["windows"] as Array<Record<string, unknown>>) ?? [];
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "planned",
        base_revision: 2,
        detail: { kind: "reconcile" },
        desired_geometry: windows.map((w) => ({
            window: w["window"],
            leaf: `${String(w["window"])}-leaf`,
            output: w["output"],
            workspace: w["workspace"],
            rect: w["rect"],
        })),
        desired_focus: {
            domain_output: (payload["domain"] as Record<string, unknown>)?.["output"],
            domain_workspace: (payload["domain"] as Record<string, unknown>)?.["workspace"],
            leaf: `${String(payload["focused_window"])}-leaf`,
        },
    });
}

function makeHarness(world: TWorld, over: {
    sendImpl?: (mover: object, output: object) => boolean;
    desktopsImpl?: (mover: object, desks: ReadonlyArray<object>) => boolean;
    tiledImpl?: (output: string, workspace: string) => boolean;
    withSend?: boolean;
} = {}): Harness {
    const h = {
        order: [] as string[],
        payloads: [] as Array<Record<string, unknown>>,
        timers: [] as Array<{ callback: () => void; cancelled: boolean }>,
        subs: new Map<string, (target?: object) => void>(),
        logs: [] as string[],
        world,
        observes: 0,
    };
    const winOf = (ref: object): TWin | null => world.wins.find((w) => w.ref === ref) ?? null;
    const idOfDesk = (obj: object): string | null => {
        for (const [id, o] of world.desks) {
            if (o === obj) {
                return id;
            }
        }
        return null;
    };
    const nameOfOutput = (obj: object): string | null => {
        for (const [name, o] of world.outputs) {
            if (o.obj === obj) {
                return name;
            }
        }
        return null;
    };
    const env = {
        callDbus: (_s: string, _p: string, _i: string, method: string, payload: string, callback: (reply: unknown) => void): void => {
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
            const parsed = JSON.parse(payload) as Record<string, unknown>;
            h.payloads.push(parsed);
            callback(plannedEcho(String(parsed["correlation_id"]), parsed));
        },
        scheduleOnce: (_delayMs: number, callback: () => void): (() => void) => {
            const entry = { callback, cancelled: false };
            h.timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message: string): void => {
            h.logs.push(message);
        },
        observe: (): PlanObserved | null => observeWorld(world, h),
        clearMaximize: (): "invoked" => "invoked",
        readBornMaximized: (): unknown => undefined,
        setGeometry: (target: object, rect: { x: number; y: number; w: number; h: number }): boolean => {
            h.order.push("setGeometry");
            const w = winOf(target);
            if (w !== null) {
                w.rect = { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
            }
            return true;
        },
        setActive: (target: object): boolean => {
            h.order.push("setActive");
            world.active = target;
            return true;
        },
        active: (): object | null => world.active,
        isDomainTiled: (output: string, workspace: string): boolean => over.tiledImpl?.(output, workspace) ?? true,
        resolveOutput: (name: string): object | null => {
            h.order.push(`resolveOutput:${name}`);
            return world.outputs.get(name)?.obj ?? null;
        },
        resolveDesktop: (workspace: string): object | null => {
            h.order.push(`resolveDesktop:${workspace}`);
            return world.desks.get(workspace) ?? null;
        },
        readOutputName: (ref: object): string | null => {
            const w = winOf(ref);
            return w === null ? null : w.output;
        },
        readDesktopIds: (ref: object): ReadonlyArray<string> | null => {
            const w = winOf(ref);
            return w === null ? null : [...w.desks];
        },
        readWindowConstraints: (ref: object) => {
            const w = winOf(ref);
            if (w?.fixedMin === undefined || w?.fixedMax === undefined) {
                return null;
            }
            return { resizeable: true, minSize: { ...w.fixedMin }, maxSize: { ...w.fixedMax } };
        },
        subscribe: (kind: string, handler: (target?: object) => void): (() => void) => {
            h.subs.set(kind as string, handler as (target?: object) => void);
            return (): void => {};
        },
    } as unknown as PlanAdapterEnv;
    if (over.withSend === false) {
        // Absent transfer primitive: no property at all.
    } else {
        (env as unknown as Record<string, unknown>)["sendClientToScreen"] = (mover: object, output: object): boolean => {
            h.order.push("sendClientToScreen");
            const w = winOf(mover);
            const name = nameOfOutput(output);
            if (w === null || name === null) {
                return false;
            }
            if (over.sendImpl !== undefined) {
                return over.sendImpl(mover, output);
            }
            w.output = name;
            return true;
        };
    }
    (env as unknown as Record<string, unknown>)["setDesktops"] = (mover: object, desks: ReadonlyArray<object>): boolean => {
        h.order.push("setDesktops");
        const w = winOf(mover);
        if (w === null) {
            return false;
        }
        if (over.desktopsImpl !== undefined) {
            return over.desktopsImpl(mover, desks);
        }
        const ids: string[] = [];
        for (const d of desks) {
            const id = idOfDesk(d);
            if (id === null) {
                return false;
            }
            ids.push(id);
        }
        w.desks = ids;
        return true;
    };
    (env as unknown as Record<string, unknown>)["readIsTransient"] = (ref: object): boolean | null => {
        const w = winOf(ref);
        if (w === null || w.transient === undefined) {
            return null;
        }
        return w.transient;
    };
    (env as unknown as Record<string, unknown>)["readShownWorkspace"] = (output: string): string | null => {
        return world.outputs.get(output)?.cur ?? null;
    };
    // Mirrors the production read: sticky (onAllDesktops) windows belong to
    // every desktop on their output.
    (env as unknown as Record<string, unknown>)["readFullscreenOnDomain"] = (output: string, workspace: string): boolean | null => {
        if (!world.outputs.has(output)) {
            return null;
        }
        for (const w of world.wins) {
            if (w.output === output && (w.desks.includes(workspace) || w.sticky) && w.fullscreen) {
                return true;
            }
        }
        return false;
    };
    const full: Harness = {
        env, order: h.order, payloads: h.payloads, timers: h.timers,
        subs: h.subs, logs: h.logs, world, observes: 0,
        byId: (id: string): TWin => {
            const w = world.wins.find((entry) => entry.id === id);
            if (w === undefined) {
                throw new Error(`missing window ${id}`);
            }
            return w;
        },
    };
    Object.defineProperty(full, "observes", { get: () => h.observes });
    return full;
}

function fire(h: Harness, kind: string, target?: object): void {
    h.subs.get(kind)?.(target);
}

function flush(h: Harness): void {
    const pending = h.timers.filter((t) => !t.cancelled);
    h.timers.length = 0;
    for (const t of pending) {
        t.callback();
    }
}

function lastPayload(h: Harness): Record<string, unknown> {
    const payload = h.payloads[h.payloads.length - 1];
    if (payload === undefined) {
        throw new Error("no planner payload dispatched");
    }
    return payload;
}

function payloadDomain(payload: Record<string, unknown>): { output: string; workspace: string } {
    const domain = payload["domain"] as Record<string, unknown>;
    return { output: String(domain["output"]), workspace: String(domain["workspace"]) };
}

function payloadIds(payload: Record<string, unknown>): string[] {
    return ((payload["windows"] as Array<Record<string, unknown>>) ?? []).map((w) => String(w["window"])).sort();
}

function anchorRound(h: Harness): void {
    // Settle the focused domain so the anchor, seen set, and applied
    // evidence exist before the birth under test.
    fire(h, "scope");
    flush(h);
}

describe("R-OUT-05 adapter integration (signal to admission)", () => {
    it("routes a pointer-output birth to the focused output with ordered writes", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_F);
        assert.deepEqual(h.byId("win-c").desks, [WS_F]);
        const writes = h.order.filter((o) => o === "sendClientToScreen" || o === "setDesktops" || o === "setActive");
        assert.deepEqual(writes, ["sendClientToScreen", "setDesktops", "setActive"]);
        const payload = lastPayload(h);
        assert.deepEqual(payloadDomain(payload), { output: OUT_F, workspace: WS_F });
        assert.ok(payloadIds(payload).includes("win-c"));
        assert.ok(h.logs.some((line) => line.includes("newcomer-route") && line.includes("outcome=routed")));
    });

    it("routes symmetrically when the focused output is the pointer side", () => {
        const world = makeWorld();
        const b = addWin(world, "win-b", OUT_P, WS_P);
        world.active = b.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_F, WS_F);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.deepEqual(h.byId("win-c").desks, [WS_P]);
        const payload = lastPayload(h);
        assert.deepEqual(payloadDomain(payload), { output: OUT_P, workspace: WS_P });
        assert.ok(payloadIds(payload).includes("win-c"));
    });

    it("refuses a birth signal naming a different ref (no arbitrary unknown routes)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", a.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
        const payload = lastPayload(h);
        assert.deepEqual(payloadDomain(payload), { output: OUT_P, workspace: WS_P });
    });

    it("refuses when the newcomer is not the active window (no steal)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        // Newcomer mapped on the pointer output without activation while the
        // old window stays active there: background open, never a route.
        const b = addWin(world, "win-b", OUT_P, WS_P);
        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = b.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
    });

    it("refuses before writes when the managed desktop is no longer shown", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        world.outputs.get(OUT_F)?.cur !== undefined && world.outputs.set(OUT_F, { cur: "ws-other", obj: world.outputs.get(OUT_F)?.obj as object });
        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
        assert.ok(!h.order.includes("setActive"));
    });

    it("refuses with zero writes when the transfer primitive is absent", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world, { withSend: false });
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.deepEqual(h.byId("win-c").desks, [WS_P]);
        assert.ok(!h.order.includes("setDesktops"));
        assert.ok(!h.order.includes("setActive"));
    });

    it("never moves transient dialogs", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        c.transient = true;
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
    });

    it("never moves fixed-hint newcomers (borderless fixed-size game shape)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        c.fixedMin = { w: 100, h: 80 };
        c.fixedMax = { w: 100, h: 80 };
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
        assert.ok(!h.order.includes("setActive"));
    });

    it("never moves born-fullscreen newcomers and never focuses them", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        c.fullscreen = true;
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setActive"));
    });

    it("refuses when a sticky fullscreen peer sits on the target (zero writes)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        // Sticky peers live on no desktop list yet belong to every desktop.
        const s = addWin(world, "win-s", OUT_F, WS_F);
        s.desks = [];
        s.sticky = true;
        s.fullscreen = true;
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
        assert.ok(!h.order.includes("setActive"));
        assert.ok(h.logs.some((line) => line.includes("newcomer-route") && line.includes("target-overlay")));
    });

    it("refuses when transient is unclassifiable (never assume non-dialog)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        c.transient = undefined;
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
    });

    it("rejects proof when the newcomer floats mid-move (no focus write)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world, {
            desktopsImpl: (mover) => {
                const w = world.wins.find((entry) => entry.ref === mover);
                if (w === undefined) {
                    return false;
                }
                // Host floats the window as it lands: the proof must fail
                // closed instead of focusing a float.
                w.desks = [WS_F];
                w.floating = true;
                return true;
            },
        });
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.ok(!h.order.includes("setActive"));
    });

    it("refuses when the target desktop holds a fullscreen overlay (D01)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        const d = addWin(world, "win-d", OUT_F, WS_F);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        // Native fullscreen entered on the managed desktop after anchoring.
        d.fullscreen = true;
        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
        assert.ok(!h.order.includes("setDesktops"));
        assert.ok(!h.order.includes("setActive"));
    });

    it("never routes while the anchored focus is a fullscreen overlay (D01)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        a.fullscreen = true;
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.ok(!h.order.includes("sendClientToScreen"));
    });

    it("re-observes after a partial write failure and never admits on stale ground", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        world.active = a.ref;
        const observesBefore = { count: 0 };
        const h = makeHarness(world, {
            desktopsImpl: () => false,
        });
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        anchorRound(h);
        const payloadsAfterAnchor = h.payloads.length;
        void observesBefore;

        const c = addWin(world, "win-c", OUT_P, WS_P);
        world.active = c.ref;
        fire(h, "added", c.ref);
        flush(h);

        // Membership failed after the output transfer: the adapter must not
        // tile the pre-write observation. Either it aborts the round or it
        // dispatches on re-observed truth, never a stale pointer admission.
        for (const payload of h.payloads.slice(payloadsAfterAnchor)) {
            const domain = payloadDomain(payload);
            if (domain.output === OUT_P) {
                assert.ok(!payloadIds(payload).includes("win-c"), "stale pointer-output admission of the newcomer");
            }
        }
        assert.ok(!h.order.includes("setActive"));
    });

    it("leaves startup adoption, view switches, and user moves alone", () => {
        // Adoption: first observation already holds windows on an output.
        {
            const world = makeWorld();
            const b = addWin(world, "win-b", OUT_P, WS_P);
            world.active = b.ref;
            const h = makeHarness(world);
            const adapter = new PlanAdapter(h.env);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            fire(h, "scope");
            flush(h);
            assert.ok(!h.order.includes("sendClientToScreen"));
            assert.deepEqual(payloadDomain(lastPayload(h)), { output: OUT_P, workspace: WS_P });
        }
        // View switch: preexisting unseen window, no birth signal.
        {
            const world = makeWorld();
            const a = addWin(world, "win-a", OUT_F, WS_F);
            const b = addWin(world, "win-b", OUT_P, WS_P);
            world.active = a.ref;
            const h = makeHarness(world);
            const adapter = new PlanAdapter(h.env);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            anchorRound(h);
            world.active = b.ref;
            fire(h, "scope");
            flush(h);
            assert.ok(!h.order.includes("sendClientToScreen"));
        }
        // User move after admission: seen ids never re-route.
        {
            const world = makeWorld();
            const a = addWin(world, "win-a", OUT_F, WS_F);
            world.active = a.ref;
            const h = makeHarness(world);
            const adapter = new PlanAdapter(h.env);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            anchorRound(h);
            const c = addWin(world, "win-c", OUT_P, WS_P);
            world.active = c.ref;
            fire(h, "added", c.ref);
            flush(h);
            assert.equal(h.byId("win-c").output, OUT_F);
            const writes = h.order.length;
            // Native user move back to the pointer output, no birth signal.
            h.byId("win-c").output = OUT_P;
            h.byId("win-c").desks = [WS_P];
            world.active = c.ref;
            fire(h, "scope");
            flush(h);
            assert.equal(h.order.length, writes, "no follow-up writes fight the user move");
        }
    });

    it("routes to the pre-debounce focus when it shifts before a birth (D06)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        const b = addWin(world, "win-b", OUT_P, WS_P);
        world.active = a.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        // Settle both domains so B is known/applied, ending settled on A.
        world.active = b.ref;
        fire(h, "scope");
        flush(h);
        world.active = a.ref;
        fire(h, "scope");
        flush(h);

        // Focus shifts A -> B without flushing; birth C lands on A's output
        // before the debounce runs. C must follow the B focus at launch.
        world.active = b.ref;
        fire(h, "activated", b.ref);
        const c = addWin(world, "win-c", OUT_F, WS_F);
        world.active = c.ref;
        fire(h, "added", c.ref);
        fire(h, "activated", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_P);
        assert.deepEqual(h.byId("win-c").desks, [WS_P]);
    });

    it("holds the pre-debounce focus for same-output births (D06 inverse)", () => {
        const world = makeWorld();
        const a = addWin(world, "win-a", OUT_F, WS_F);
        const b = addWin(world, "win-b", OUT_P, WS_P);
        world.active = b.ref;
        const h = makeHarness(world);
        const adapter = new PlanAdapter(h.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        // Settle both domains so A is known/applied, ending settled on B.
        world.active = a.ref;
        fire(h, "scope");
        flush(h);
        world.active = b.ref;
        fire(h, "scope");
        flush(h);

        // Focus shifts B -> A without flushing; birth C lands on A's output,
        // the same output as the true pre-birth focus, so it must stay.
        world.active = a.ref;
        fire(h, "activated", a.ref);
        const c = addWin(world, "win-c", OUT_F, WS_F);
        world.active = c.ref;
        fire(h, "added", c.ref);
        fire(h, "activated", c.ref);
        flush(h);

        assert.equal(h.byId("win-c").output, OUT_F);
        assert.deepEqual(h.byId("win-c").desks, [WS_F]);
        assert.ok(!h.order.includes("sendClientToScreen"));
    });
});
