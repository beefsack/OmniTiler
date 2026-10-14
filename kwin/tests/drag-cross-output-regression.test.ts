import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    PLAN_DEBOUNCE_MS,
    PLAN_METHOD,
    PlanAdapter,
    type PlanAdapterEnv,
    type PlanDragPreviewResult,
    type PlanObserved,
    type PlanObservedWindow,
} from "../src/plan-adapter";

// Exact report geometry: L logical 2048x1152, R 1920x1080 at offset (2048,72).
const L_BOUNDS = { x: 0, y: 0, w: 2048, h: 1152 };
const R_BOUNDS = { x: 2048, y: 72, w: 1920, h: 1080 };
const OUT_L = "out-L";
const OUT_R = "out-R";
const WS = "ws-1";

interface Refs {
    readonly mover: object;
    readonly sib: object;
    readonly dest: object;
}

function makeRefs(): Refs {
    return { mover: {}, sib: {}, dest: {} };
}

function win(id: string, ref: object, output: string, rect: { x: number; y: number; w: number; h: number }): PlanObservedWindow {
    return {
        id,
        ref,
        rect: { ...rect },
        output,
        workspace: WS,
        fullscreen: false,
        maximized: false,
        resourceClass: "unknown",
    };
}

function domain(output: string, bounds: { x: number; y: number; w: number; h: number }, focusedId: string, windows: PlanObservedWindow[], activeRef: object, fp: string): PlanObserved {
    return {
        domainOutput: output,
        domainWorkspace: WS,
        domainBounds: { ...bounds },
        domainGap: 8,
        domainOuterGap: 8,
        focusedId,
        windows: Object.freeze(windows.map((entry) => Object.freeze(entry))),
        activeRef,
        fingerprint: fp,
        revalidate: () => true,
    };
}

interface Call {
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface Mocks {
    readonly previewCalls: Call[];
    readonly dbusCalls: Call[];
    readonly logs: string[];
    readonly geometries: Array<{ target: object; rect: { x: number; y: number; w: number; h: number } }>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    observeImpl: () => PlanObserved | null;
    hiddenImpl: () => ReadonlyArray<PlanObserved>;
    interactiveImpl: () => boolean;
    readonly env: PlanAdapterEnv;
}

function mockEnv(refs: Refs): Mocks {
    const previewCalls: Call[] = [];
    const dbusCalls: Call[] = [];
    const logs: string[] = [];
    const geometries: Mocks["geometries"] = [];
    const timers: Mocks["timers"] = [];
    const state = {
        previewCalls,
        dbusCalls,
        logs,
        geometries,
        timers,
        observeImpl: (): PlanObserved | null => null,
        hiddenImpl: (): ReadonlyArray<PlanObserved> => [],
        interactiveImpl: (): boolean => false,
        env: null as unknown as PlanAdapterEnv,
    };
    const env: PlanAdapterEnv = {
        callDbus: (service, _path, _iface, method, payload, callback): void => {
            void service;
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "NameHasOwner" || method === "StartServiceByName") {
                callback(true);
                return;
            }
            if (method !== PLAN_METHOD) {
                return;
            }
            try {
                const parsed = JSON.parse(payload) as Record<string, unknown>;
                const command = parsed["command"] as Record<string, unknown>;
                if (command?.["op"] === "drag-preview") {
                    previewCalls.push({ payload, callback });
                    return;
                }
            } catch (error) {
                void error;
            }
            dbusCalls.push({ payload, callback });
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const entry = { delayMs, callback, cancelled: false };
            timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message): void => {
            logs.push(message);
        },
        observe: (): PlanObserved | null => state.observeImpl(),
        observeHidden: (): ReadonlyArray<PlanObserved> => state.hiddenImpl(),
        isInteractiveResizeActive: (): boolean => state.interactiveImpl(),
        clearMaximize: (): "invoked" => "invoked",
        setGeometry: (target, rect): boolean => {
            geometries.push({ target, rect: { ...rect } });
            return true;
        },
        setActive: (): boolean => true,
        active: () => refs.mover,
        subscribe: (): (() => void) => (): void => {},
    };
    (state as { env: PlanAdapterEnv }).env = env;
    return state as Mocks;
}

function enableAdapter(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
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

function corrOf(call: Call): string {
    return (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
}

function previewReply(correlation: string, scopeOutput: string): string {
    return JSON.stringify({
        v: 1, correlation_id: correlation, outcome: "preview", kind: "drag-preview", base_revision: 2,
        detail: { kind: "drag-preview", capability: "place-tiled" },
        preview_rect: { x: 2048, y: 72, w: 960, h: 1008 },
        hover_prior: { domain_output: scopeOutput, domain_workspace: WS, source_leaf: "win-a-leaf", source_window: "win-a", revision: 2, prior: null },
    });
}

function plannedReply(correlation: string, output: string, geom: Array<{ window: string; rect: { x: number; y: number; w: number; h: number } }>): string {
    return JSON.stringify({
        v: 1, correlation_id: correlation, outcome: "planned", base_revision: 2,
        detail: { kind: "reconcile" },
        desired_geometry: geom.map((entry) => ({
            window: entry.window, leaf: `${entry.window}-leaf`, output, workspace: WS, rect: entry.rect,
        })),
        desired_focus: { domain_output: output, domain_workspace: WS, leaf: `${geom[0]?.window ?? "win-x"}-leaf` },
    });
}

function planPayload(call: Call): Record<string, unknown> {
    return JSON.parse(call.payload) as Record<string, unknown>;
}

function planOp(call: Call): string {
    return String((planPayload(call)["command"] as Record<string, unknown>)["op"]);
}

function planDomain(call: Call): string {
    return String((planPayload(call)["domain"] as Record<string, unknown>)["output"]);
}

function describeCalls(mocks: Mocks): string {
    return mocks.dbusCalls.map((call) => `${planOp(call)}@${planDomain(call)}`).join(",");
}

function busyCount(mocks: Mocks, _drag: string): number {
    void _drag;
    return mocks.logs.filter((line) => line.includes("busy-refused") && line.includes("kind=drag-preview")).length;
}

function backedOffCount(mocks: Mocks, drag: string): number {
    return mocks.logs.filter((line) => line.includes("drag-preview-backed-off") && line.includes(`correlation=${drag}`)).length;
}

describe("cross-output drag regression (exact L/R geometry)", () => {
    it("L->R crossing dispatches when the mover already lives natively on R", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // Foreground stays L (another window active) while KWin already moved
        // win-a to R mid-drag.
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
        ], refs.sib, "fp-L-no-mover");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
            win("win-a", refs.mover, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
        ], refs.dest, "fp-R-with-mover")];
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        const ok = adapter.requestDragPreview("win-a", 2500, 500, "drag-70", { output: OUT_L, workspace: WS }, (r) => seen.push(r));
        assert.equal(ok, true, `expected dispatch, logs: ${mocks.logs.join(" | ")}`);
        assert.equal(mocks.previewCalls.length, 1);
        const payload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], OUT_R);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_R));
        assert.equal(seen.length, 1);
        assert.ok(seen[0] !== null, "native-on-destination reply still applies");
        adapter.clearDragPreview("drag-70");
    });

    it("R->L crossing dispatches when the mover already lives natively on L", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
        ], refs.dest, "fp-R-no-mover");
        mocks.hiddenImpl = () => [domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
        ], refs.sib, "fp-L-with-mover")];
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        const ok = adapter.requestDragPreview("win-a", 500, 500, "drag-71", { output: OUT_R, workspace: WS }, (r) => seen.push(r));
        assert.equal(ok, true, `expected dispatch, logs: ${mocks.logs.join(" | ")}`);
        const payload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], OUT_L);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_L));
        assert.ok(seen[0] !== null);
        adapter.clearDragPreview("drag-71");
    });

    it("same-output control still dispatches from the source domain", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-72", { output: OUT_L, workspace: WS }, (r) => seen.push(r)), true);
        const payload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], OUT_L);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_L));
        assert.ok(seen[0] !== null);
        adapter.clearDragPreview("drag-72");
    });

    it("preview scope stays live when the mover arrives natively on the destination before the reply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let moverOnDest = false;
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => {
            if (!moverOnDest) {
                return [domain(OUT_R, R_BOUNDS, "win-c", [
                    win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
                ], refs.dest, "fp-R")];
            }
            return [domain(OUT_R, R_BOUNDS, "win-c", [
                win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
                win("win-a", refs.mover, OUT_R, { x: 3008, y: 72, w: 960, h: 1008 }),
            ], refs.dest, "fp-R-mover")];
        };
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 2500, 500, "drag-73", { output: OUT_L, workspace: WS }, (r) => seen.push(r)), true);
        moverOnDest = true;
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 2048, h: 1152 }),
        ], refs.sib, "fp-L-mover-gone");
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_R));
        assert.equal(seen.length, 1);
        assert.ok(seen[0] !== null, "mover-on-destination arrival keeps the scope live");
        adapter.clearDragPreview("drag-73");
    });

    it("repeated identical preview refusals log once until a dispatch resets", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => null;
        mocks.hiddenImpl = () => [];
        const adapter = enableAdapter(mocks);
        for (let i = 0; i < 5; i += 1) {
            assert.equal(adapter.requestDragPreview("win-a", 2500, 500, "drag-74", { output: OUT_L, workspace: WS }, () => {}), false);
        }
        const refused = mocks.logs.filter((line) => line.includes("drag-preview-refused") && line.includes("correlation=drag-74"));
        assert.equal(refused.length, 1, `expected one refused log, got ${refused.length}: ${mocks.logs.join(" | ")}`);
        // A dispatch resets the key so the next identical refusal logs again.
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-74", { output: OUT_L, workspace: WS }, () => {}), true);
        mocks.observeImpl = () => null;
        assert.equal(adapter.requestDragPreview("win-a", 2500, 500, "drag-74", { output: OUT_L, workspace: WS }, () => {}), false);
        const refusedAfter = mocks.logs.filter((line) => line.includes("drag-preview-refused") && line.includes("correlation=drag-74"));
        assert.equal(refusedAfter.length, 2);
        adapter.clearDragPreview("drag-74");
    });

    it("active drag defers foreground and hidden reconcile dispatch on both outputs", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        mocks.interactiveImpl = () => true;
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 0, `no reconcile while dragging, got ${mocks.dbusCalls.length}`);
        mocks.interactiveImpl = () => false;
        adapter.requestResync();
        runDebounce(mocks);
        assert.ok(mocks.dbusCalls.length >= 1, "reconcile dispatches once the drag hold releases");
    });

    it("already-in-flight reconcile replies never actuate mid-drag", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "foreground reconcile dispatched before the drag");
        const flight = mocks.dbusCalls[0] as Call;
        const flightCorr = corrOf(flight);
        mocks.interactiveImpl = () => true;
        flight.callback(plannedReply(flightCorr, OUT_L, [
            { window: "win-a", rect: { x: 0, y: 0, w: 1024, h: 1152 } },
            { window: "win-b", rect: { x: 1024, y: 0, w: 1024, h: 1152 } },
        ]));
        assert.equal(mocks.geometries.length, 0, "in-flight reconcile reply writes nothing mid-drag");
        assert.ok(!mocks.logs.some((line) => line.includes(`cmd=${flightCorr}`) && line.includes("outcome=planned-applied")));
    });

    it("hidden drift holds during drag and dispatches after release", () => {
        const refs = makeRefs();
        const extraRef: object = {};
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "foreground admission dispatches first");
        const fg = mocks.dbusCalls[0] as Call;
        fg.callback(plannedReply(corrOf(fg), OUT_L, [
            { window: "win-a", rect: { x: 0, y: 0, w: 1024, h: 1152 } },
            { window: "win-b", rect: { x: 1024, y: 0, w: 1024, h: 1152 } },
        ]));
        adapter.requestResync();
        runDebounce(mocks);
        const hiddenFirst = mocks.dbusCalls.filter((call) => planDomain(call) === OUT_R);
        assert.equal(hiddenFirst.length, 1, `hidden admission converges, got: ${describeCalls(mocks)}`);
        const hid = hiddenFirst[0] as Call;
        hid.callback(plannedReply(corrOf(hid), OUT_R, [
            { window: "win-c", rect: { x: 2048, y: 72, w: 1920, h: 1008 } },
        ]));
        // Real hidden drift: a newcomer lands on R while the drag is active.
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
            win("win-d", extraRef, OUT_R, { x: 3008, y: 72, w: 960, h: 1008 }),
        ], refs.dest, "fp-R-drift")];
        mocks.interactiveImpl = () => true;
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(
            mocks.dbusCalls.filter((call) => planDomain(call) === OUT_R).length,
            1,
            `hidden drift stays held mid-drag, got: ${describeCalls(mocks)}`,
        );
        mocks.interactiveImpl = () => false;
        adapter.requestResync();
        runDebounce(mocks);
        const hiddenAfter = mocks.dbusCalls.filter((call) => planDomain(call) === OUT_R);
        assert.equal(hiddenAfter.length, 2, `drift dispatches after release, got: ${describeCalls(mocks)}`);
        const last = hiddenAfter[hiddenAfter.length - 1] as Call;
        assert.equal(planOp(last), "reconcile");
    });

    it("hidden in-flight reconcile reply writes nothing mid-drag and recovers after release", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        const fg = mocks.dbusCalls[0] as Call;
        fg.callback(plannedReply(corrOf(fg), OUT_L, [
            { window: "win-a", rect: { x: 0, y: 0, w: 1024, h: 1152 } },
            { window: "win-b", rect: { x: 1024, y: 0, w: 1024, h: 1152 } },
        ]));
        adapter.requestResync();
        runDebounce(mocks);
        const hiddenFirst = mocks.dbusCalls.filter((call) => planDomain(call) === OUT_R);
        assert.equal(hiddenFirst.length, 1, `hidden flight dispatched, got: ${describeCalls(mocks)}`);
        const hid = hiddenFirst[0] as Call;
        const hidCorr = corrOf(hid);
        const destWritesBefore = mocks.geometries.filter((entry) => entry.target === refs.dest).length;
        mocks.interactiveImpl = () => true;
        hid.callback(plannedReply(hidCorr, OUT_R, [
            { window: "win-c", rect: { x: 2048, y: 72, w: 1800, h: 1000 } },
        ]));
        assert.equal(
            mocks.geometries.filter((entry) => entry.target === refs.dest).length,
            destWritesBefore,
            "hidden reply writes no destination geometry mid-drag",
        );
        assert.ok(!mocks.logs.some((line) => line.includes(`cmd=${hidCorr}`) && line.includes("outcome=planned-applied")));
        mocks.interactiveImpl = () => false;
        adapter.requestResync();
        runDebounce(mocks);
        const hiddenAfter = mocks.dbusCalls.filter((call) => planDomain(call) === OUT_R);
        assert.equal(hiddenAfter.length, 2, `hidden flight recovers after release, got: ${describeCalls(mocks)}`);
        const retry = hiddenAfter[hiddenAfter.length - 1] as Call;
        const retryCorr = corrOf(retry);
        retry.callback(plannedReply(retryCorr, OUT_R, [
            { window: "win-c", rect: { x: 2048, y: 72, w: 1800, h: 1000 } },
        ]));
        assert.equal(
            mocks.geometries.filter((entry) => entry.target === refs.dest).length,
            destWritesBefore + 1,
            "recovered hidden flight actuates after release",
        );
        assert.ok(mocks.logs.some((line) => line.includes(`cmd=${retryCorr}`) && line.includes("outcome=planned-applied")));
    });

    it("mover in zero or several hidden domains stays fail-closed absent", () => {
        const refs = makeRefs();
        const otherRef: object = {};
        // Zero holders: mover in neither foreground nor hidden.
        const mocksZero = mockEnv(refs);
        mocksZero.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 2048, h: 1152 }),
        ], refs.sib, "fp-L-no-mover");
        mocksZero.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapterZero = enableAdapter(mocksZero);
        const seenZero: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapterZero.requestDragPreview("win-a", 2500, 500, "drag-76", { output: OUT_L, workspace: WS }, (r) => seenZero.push(r)), false);
        assert.deepEqual(seenZero, [null]);
        assert.equal(mocksZero.previewCalls.length, 0);
        assert.ok(mocksZero.logs.some((line) => line.includes("drag-preview-refused") && line.includes("correlation=drag-76") && line.includes("reason=absent")));
        // Several holders: mover claimed by two hidden domains.
        const mocksMulti = mockEnv(refs);
        mocksMulti.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 2048, h: 1152 }),
        ], refs.sib, "fp-L-no-mover");
        mocksMulti.hiddenImpl = () => [
            domain(OUT_R, R_BOUNDS, "win-c", [
                win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
                win("win-a", refs.mover, OUT_R, { x: 3008, y: 72, w: 960, h: 1008 }),
            ], refs.dest, "fp-R-mover"),
            domain("out-X", { x: 0, y: 1200, w: 1920, h: 1080 }, "win-x", [
                win("win-x", otherRef, "out-X", { x: 0, y: 1200, w: 1920, h: 1080 }),
                win("win-a", refs.mover, "out-X", { x: 0, y: 1200, w: 960, h: 1080 }),
            ], otherRef, "fp-X-mover"),
        ];
        const adapterMulti = enableAdapter(mocksMulti);
        const seenMulti: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapterMulti.requestDragPreview("win-a", 2500, 500, "drag-77", { output: OUT_L, workspace: WS }, (r) => seenMulti.push(r)), false);
        assert.deepEqual(seenMulti, [null]);
        assert.equal(mocksMulti.previewCalls.length, 0);
        assert.ok(mocksMulti.logs.some((line) => line.includes("drag-preview-refused") && line.includes("correlation=drag-77") && line.includes("reason=absent")));
    });

    it("busy preview backoff logs once until a dispatch resets the key", () => {
        const refs = makeRefs();
        const extraRef: object = {};
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(mocks.dbusCalls.length, 1, "occupying flight dispatched");
        for (let i = 0; i < 5; i += 1) {
            assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-78", { output: OUT_L, workspace: WS }, () => {}), false);
        }
        assert.equal(busyCount(mocks, "drag-78"), 1, `busy-refused once, got: ${mocks.logs.join(" | ")}`);
        assert.equal(backedOffCount(mocks, "drag-78"), 1);
        const fg = mocks.dbusCalls[0] as Call;
        fg.callback(plannedReply(corrOf(fg), OUT_L, [
            { window: "win-a", rect: { x: 0, y: 0, w: 1024, h: 1152 } },
            { window: "win-b", rect: { x: 1024, y: 0, w: 1024, h: 1152 } },
        ]));
        adapter.requestResync();
        runDebounce(mocks);
        for (const call of mocks.dbusCalls.filter((entry) => planDomain(entry) === OUT_R)) {
            (call as Call).callback(plannedReply(corrOf(call as Call), OUT_R, [
                { window: "win-c", rect: { x: 2048, y: 72, w: 1920, h: 1008 } },
            ]));
        }
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-78", { output: OUT_L, workspace: WS }, (r) => seen.push(r)), true);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_L));
        assert.ok(seen[0] !== null);
        // Occupy the flight again with a real foreground change.
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
            win("win-d", extraRef, OUT_L, { x: 0, y: 576, w: 2048, h: 576 }),
        ], refs.mover, "fp-L-newcomer");
        adapter.requestResync();
        runDebounce(mocks);
        assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-78", { output: OUT_L, workspace: WS }, () => {}), false);
        assert.equal(busyCount(mocks, "drag-78"), 2, "backoff logs again after a dispatch reset the key");
        assert.equal(backedOffCount(mocks, "drag-78"), 2);
        adapter.clearDragPreview("drag-78");
    });

    it("drop from an absent foreground resolves the native destination with its source binding", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 2048, h: 1152 }),
        ], refs.sib, "fp-L-no-mover");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
            win("win-a", refs.mover, OUT_R, { x: 3008, y: 72, w: 960, h: 1008 }),
        ], refs.dest, "fp-R-with-mover")];
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestDragDrop("win-a", 2500, 500, "drag-80", { output: OUT_L, workspace: WS }), true);
        assert.equal(mocks.dbusCalls.length, 1, `one drop intent, no separate source removal: ${describeCalls(mocks)}`);
        const drop = mocks.dbusCalls[0] as Call;
        const payload = JSON.parse(drop.payload) as Record<string, unknown>;
        const command = payload["command"] as Record<string, unknown>;
        assert.equal(command["op"], "drag-drop");
        assert.equal(command["source_output"], OUT_L);
        assert.equal(command["source_workspace"], WS);
        assert.ok(!("removed" in command), "source removal rides topology, not a removed field");
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], OUT_R);
        assert.equal(payload["focused_window"], "win-a");
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.find((entry) => entry["window"] === "win-a")?.["output"], OUT_R);
        assert.ok(windows.some((entry) => entry["window"] === "win-c"), "destination sibling retained");
        assert.ok(mocks.logs.some((line) => line.includes("drag-drop-cross-output") && line.includes("correlation=drag-80")));
    });

    it("preview reply stays live when the foreground switches to the destination before the mover arrives", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // Dispatch while the mover is still native on L (foreground L).
        mocks.observeImpl = () => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        mocks.hiddenImpl = () => [domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R")];
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 2500, 500, "drag-79", { output: OUT_L, workspace: WS }, (r) => seen.push(r)), true);
        // Foreground follows the pointer to R before the mover arrives
        // natively: the mover still lives on hidden L mid-transfer.
        mocks.observeImpl = () => domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R-fg");
        mocks.hiddenImpl = () => [domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L-mover")];
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_R));
        assert.equal(seen.length, 1, "in-flight reply still settles");
        assert.ok(seen[0] !== null, "destination scope stays live while the mover is mid-transfer, not stale-scope");
        assert.ok(!mocks.logs.some((line) => line.includes("drag-preview-settled") && line.includes("correlation=drag-79") && line.includes("stale-scope")));
        adapter.clearDragPreview("drag-79");
    });

    it("right-then-left previews bind the correct logical output domain, bounds, and target scope", () => {
        // Mixed scale, vertically offset logical fixtures: small 1280x720 top
        // output beside a large 4k output dropped 400px lower.
        const topBounds = { x: 0, y: 0, w: 1280, h: 720 };
        const lowBounds = { x: 1280, y: 400, w: 3840, h: 2160 };
        const outTop = "out-T";
        const outLow = "out-B";
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const topDomain = (): PlanObserved => domain(outTop, topBounds, "win-a", [
            win("win-a", refs.mover, outTop, { x: 0, y: 0, w: 640, h: 720 }),
            win("win-b", refs.sib, outTop, { x: 640, y: 0, w: 640, h: 720 }),
        ], refs.mover, "fp-top");
        const lowDomain = (): PlanObserved => domain(outLow, lowBounds, "win-c", [
            win("win-c", refs.dest, outLow, { x: 1280, y: 400, w: 3840, h: 2088 }),
        ], refs.dest, "fp-low");
        mocks.observeImpl = topDomain;
        mocks.hiddenImpl = () => [lowDomain()];
        const adapter = enableAdapter(mocks);
        // Right: pointer over the low output resolves the low domain.
        const seenRight: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 2000, 1200, "drag-81", { output: outTop, workspace: WS }, (r) => seenRight.push(r)), true);
        const rightPayload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((rightPayload["domain"] as Record<string, unknown>)["output"], outLow);
        assert.deepEqual((rightPayload["domain"] as Record<string, unknown>)["bounds"], lowBounds);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), outLow));
        assert.ok(seenRight[0] !== null);
        adapter.clearDragPreview("drag-81");
        // Then left: pointer back over the top output resolves the top domain.
        const seenLeft: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 500, 300, "drag-82", { output: outTop, workspace: WS }, (r) => seenLeft.push(r)), true);
        const leftPayload = JSON.parse(mocks.previewCalls[1]?.payload as string) as Record<string, unknown>;
        assert.equal((leftPayload["domain"] as Record<string, unknown>)["output"], outTop);
        assert.deepEqual((leftPayload["domain"] as Record<string, unknown>)["bounds"], topBounds);
        mocks.previewCalls[1]?.callback(previewReply(corrOf(mocks.previewCalls[1] as Call), outTop));
        assert.ok(seenLeft[0] !== null);
        adapter.clearDragPreview("drag-82");
    });

    it("same drag re-crosses right-then-left after the native output flips", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const leftWithMover = (): PlanObserved => domain(OUT_L, L_BOUNDS, "win-a", [
            win("win-a", refs.mover, OUT_L, { x: 0, y: 0, w: 1024, h: 1152 }),
            win("win-b", refs.sib, OUT_L, { x: 1024, y: 0, w: 1024, h: 1152 }),
        ], refs.mover, "fp-L");
        const rightAlone = (): PlanObserved => domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 1920, h: 1008 }),
        ], refs.dest, "fp-R");
        mocks.observeImpl = leftWithMover;
        mocks.hiddenImpl = () => [rightAlone()];
        const adapter = enableAdapter(mocks);
        // Right: the same drag-83 gesture samples over R.
        const seenRight: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 2500, 500, "drag-83", { output: OUT_L, workspace: WS }, (r) => seenRight.push(r)), true);
        const rightPayload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((rightPayload["domain"] as Record<string, unknown>)["output"], OUT_R);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as Call), OUT_R));
        assert.ok(seenRight[0] !== null);
        // Native output flips mid-gesture: the mover now lives on R and the
        // foreground follows it there.
        mocks.observeImpl = () => domain(OUT_R, R_BOUNDS, "win-c", [
            win("win-c", refs.dest, OUT_R, { x: 2048, y: 72, w: 960, h: 1008 }),
            win("win-a", refs.mover, OUT_R, { x: 3008, y: 72, w: 960, h: 1008 }),
        ], refs.dest, "fp-R-mover");
        mocks.hiddenImpl = () => [domain(OUT_L, L_BOUNDS, "win-b", [
            win("win-b", refs.sib, OUT_L, { x: 0, y: 0, w: 2048, h: 1152 }),
        ], refs.sib, "fp-L-sib")];
        // Left: the same drag-83 correlation samples back over L.
        const seenLeft: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 500, 500, "drag-83", { output: OUT_L, workspace: WS }, (r) => seenLeft.push(r)), true);
        const leftCall = mocks.previewCalls[mocks.previewCalls.length - 1] as Call;
        const leftPayload = JSON.parse(leftCall.payload) as Record<string, unknown>;
        assert.equal((leftPayload["domain"] as Record<string, unknown>)["output"], OUT_L, "re-cross resolves the left domain");
        assert.deepEqual((leftPayload["domain"] as Record<string, unknown>)["bounds"], L_BOUNDS, "re-cross carries the left bounds");
        assert.equal(((leftPayload["command"] as Record<string, unknown>)["source_output"]), OUT_L, "Started source binding retained");
        const leftRect = { x: 0, y: 0, w: 1024, h: 1080 };
        leftCall.callback(JSON.stringify({
            v: 1, correlation_id: corrOf(leftCall), outcome: "preview", kind: "drag-preview", base_revision: 2,
            detail: { kind: "drag-preview", capability: "place-tiled" },
            preview_rect: leftRect,
            hover_prior: { domain_output: OUT_L, domain_workspace: WS, source_leaf: "win-a-leaf", source_window: "win-a", revision: 2, prior: null },
        }));
        assert.equal(seenLeft.length, 1);
        assert.ok(seenLeft[0] !== null, "re-cross reply applies in the left scope");
        assert.deepEqual(seenLeft[0]?.rect, leftRect);
        assert.ok(leftRect.x >= L_BOUNDS.x && leftRect.y >= L_BOUNDS.y && leftRect.x + leftRect.w <= L_BOUNDS.x + L_BOUNDS.w && leftRect.y + leftRect.h <= L_BOUNDS.y + L_BOUNDS.h, "preview rect inside the left output bounds");
        adapter.clearDragPreview("drag-83");
    });
});
