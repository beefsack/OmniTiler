import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    PLAN_METHOD,
    PlanAdapter,
    type PlanAdapterEnv,
    type PlanDragPreviewResult,
    type PlanObserved,
    type PlanObservedWindow,
} from "../src/plan-adapter";

function makeRefs(): { a: object; b: object } {
    return { a: {}, b: {} };
}

interface Rect {
    readonly x: number;
    readonly y: number;
    readonly w: number;
    readonly h: number;
}

function tile(id: string, ref: object, output: string, rect: Rect, extra: Partial<PlanObservedWindow> = {}): PlanObservedWindow {
    return {
        id,
        ref,
        rect: { ...rect },
        output,
        workspace: "ws-1",
        fullscreen: false,
        maximized: false,
        resourceClass: "unknown",
        ...extra,
    };
}

interface PreviewCall {
    readonly service: string;
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface Mocks {
    readonly previewCalls: PreviewCall[];
    readonly planCalls: PreviewCall[];
    readonly logs: string[];
    observeImpl: () => PlanObserved | null;
    readonly env: PlanAdapterEnv;
}

function mockEnv(refs: { a: object; b: object }): Mocks {
    const previewCalls: PreviewCall[] = [];
    const planCalls: PreviewCall[] = [];
    const logs: string[] = [];
    const state: Mocks = {
        previewCalls,
        planCalls,
        logs,
        observeImpl: () => makeObserved(refs),
        env: null as unknown as PlanAdapterEnv,
    };
    const env: PlanAdapterEnv = {
        callDbus: (service, _path, _iface, method, payload, callback): void => {
            const handshake: Record<string, unknown> = { NameHasOwner: true, GetNameOwner: ":1.7", StartServiceByName: 1 };
            if (method in handshake) {
                callback(handshake[method]);
                return;
            }
            if (method === PLAN_METHOD && !state.env) {
                throw new Error("unreachable");
            }
            try {
                const parsed = JSON.parse(payload) as Record<string, unknown>;
                const command = parsed["command"] as Record<string, unknown>;
                if (command?.["op"] === "drag-preview") {
                    previewCalls.push({ service, method, payload, callback });
                    return;
                }
            } catch (error) {
                void error;
            }
            planCalls.push({ service, method, payload, callback });
        },
        scheduleOnce: (_delayMs, _callback): (() => void) => (): void => {},
        log: (message): void => {
            logs.push(message);
        },
        observe: (): PlanObserved | null => state.observeImpl(),
        clearMaximize: (): "invoked" => "invoked",
        setGeometry: (): boolean => true,
        setActive: (): boolean => true,
        active: () => refs.a,
        readWindowConstraints: (ref): { resizeable: boolean; minSize: { w: number; h: number } | null; maxSize: { w: number; h: number } | null } | null => {
            if (ref === refs.a) {
                return { resizeable: true, minSize: { w: 100, h: 100 }, maxSize: { w: 800, h: 600 } };
            }
            return null;
        },
        subscribe: (): (() => void) => (): void => {},
    };
    (state as { env: PlanAdapterEnv }).env = env;
    return state;
}

function makeObserved(refs: { a: object; b: object }): PlanObserved {
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-1",
        domainBounds: { x: 0, y: 0, w: 1600, h: 1000 },
        domainGap: 8,
        domainOuterGap: 8,
        focusedId: "win-a",
        windows: Object.freeze([
            Object.freeze(tile("win-a", refs.a, "out-1", { x: 0, y: 0, w: 600, h: 800 })),
            Object.freeze(tile("win-b", refs.b, "out-1", { x: 600, y: 0, w: 600, h: 800 })),
        ]),
        activeRef: refs.a,
        fingerprint: "fp-1",
        revalidate: () => true,
    };
}

function makeDest(
    destRef: object,
    extra: Partial<PlanObservedWindow> = {},
    rect: Rect = { x: 1600, y: 0, w: 600, h: 800 },
    moverRef: object | null = null,
): PlanObserved {
    const windows =
        moverRef === null
            ? [tile("win-c", destRef, "out-2", rect, extra)]
            : [tile("win-c", destRef, "out-2", rect, extra), tile("win-a", moverRef, "out-2", { x: 0, y: 0, w: 600, h: 800 })];
    return {
        domainOutput: "out-2",
        domainWorkspace: "ws-1",
        domainBounds: { x: 1600, y: 0, w: 1200, h: 800 },
        domainGap: 8,
        domainOuterGap: 8,
        focusedId: "win-c",
        windows: Object.freeze(windows.map((entry) => Object.freeze(entry))),
        activeRef: destRef,
        fingerprint: moverRef === null ? "fp-dest" : "fp-dest-with-mover",
        revalidate: () => true,
    };
}

function enableAdapter(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function previewReply(correlation: string): string {
    return JSON.stringify({
        v: 1, correlation_id: correlation, outcome: "preview", kind: "drag-preview", base_revision: 2,
        detail: { kind: "drag-preview", capability: "place-tiled" },
        preview_rect: { x: 600, y: 0, w: 600, h: 800 },
        hover_prior: { domain_output: "out-1", domain_workspace: "ws-1", source_leaf: "win-a-leaf", source_window: "win-a", revision: 2, prior: null },
    });
}

function rejectedReply(correlation: string, kind: string): string {
    return JSON.stringify({ v: 1, correlation_id: correlation, outcome: "rejected", kind });
}

function plannedCrossReply(correlation: string): string {
    return JSON.stringify({
        v: 1, correlation_id: correlation, outcome: "planned", base_revision: 2,
        detail: { kind: "drag-drop", capability: "place-tiled" },
        desired_geometry: [
            { window: "win-a", leaf: "win-a-leaf", output: "out-2", workspace: "ws-1", rect: { x: 1600, y: 0, w: 600, h: 800 } },
            { window: "win-c", leaf: "win-c-leaf", output: "out-2", workspace: "ws-1", rect: { x: 2200, y: 0, w: 600, h: 800 } },
        ],
        desired_focus: { domain_output: "out-2", domain_workspace: "ws-1", leaf: "win-a-leaf" },
    });
}

function corrOf(call: PreviewCall): string {
    return (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
}

function commandOf(call: PreviewCall): Record<string, unknown> {
    return (JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
}

function installTransfer(
    mocks: Mocks,
    opts: {
        readOutput?: string | null;
        readDesktops?: ReadonlyArray<string> | null;
        onTransfer?: () => void;
        countGeometries?: { count: number };
    } = {},
): string[] {
    const out2 = { name: "out-2" };
    const ws1 = { id: "ws-1" };
    const events: string[] = [];
    const extra = mocks.env as unknown as Record<string, unknown>;
    const geometries = opts.countGeometries;
    const baseSetGeometry = mocks.env.setGeometry;
    extra["resolveOutput"] = (): object | null => out2;
    extra["resolveDesktop"] = (): object | null => ws1;
    extra["sendClientToScreen"] = (): boolean => {
        events.push("transfer");
        opts.onTransfer?.();
        return true;
    };
    extra["setDesktops"] = (): boolean => {
        events.push("membership");
        return true;
    };
    extra["readOutputName"] = (): string | null => opts.readOutput ?? "out-2";
    extra["readDesktopIds"] = (): ReadonlyArray<string> | null => opts.readDesktops ?? ["ws-1"];
    extra["readGeometry"] = (): { x: number; y: number; w: number; h: number } | null => ({ x: 0, y: 0, w: 10, h: 10 });
    extra["subscribeMoverOutput"] = (): (() => void) | null => (): void => {};
    extra["subscribeMoverDesktops"] = (): (() => void) | null => (): void => {};
    extra["subscribeWindowGeometry"] = (): (() => void) | null => (): void => {};
    extra["setGeometry"] = (target: object, rect: { x: number; y: number; w: number; h: number }): boolean => {
        events.push("geometry");
        if (geometries) {
            geometries.count += 1;
        }
        return baseSetGeometry(target, rect);
    };
    return events;
}

function assertNoRawIds(logs: string[], ids: string[]): void {
    for (const line of logs) {
        for (const id of ids) {
            assert.ok(!line.includes(id), `raw id leaked: ${line}`);
        }
    }
}

describe("bounded read-only drag preview", () => {
    it("hint-bearing preview sticks the prior into the final drop; normal logs hide raw ids", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-20", { output: "out-9", workspace: "ws-9" }, (r) => seen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 1);
        assert.equal(mocks.planCalls.length, 0);
        const command = commandOf(mocks.previewCalls[0] as PreviewCall);
        assert.equal(command["op"], "drag-preview");
        assert.equal(command["window"], "win-a");
        assert.equal(command["source_output"], "out-9");
        assert.equal(command["source_workspace"], "ws-9");
        assert.ok(!("hover_prior" in command), "no prior before the first preview");
        const payload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual((windows.find((e) => e["window"] === "win-a") as Record<string, unknown>)["min_size"], { w: 100, h: 100 });
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as PreviewCall)));
        assert.equal(seen.length, 1);
        assert.deepEqual(seen[0]?.rect, { x: 600, y: 0, w: 600, h: 800 });
        // The carried prior rides the next preview verbatim.
        const seen2: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-20", undefined, (r) => seen2.push(r)), true);
        assert.deepEqual(commandOf(mocks.previewCalls[1] as PreviewCall)["hover_prior"], (seen[0] as PlanDragPreviewResult).hoverPrior);
        mocks.previewCalls[1]?.callback(previewReply(corrOf(mocks.previewCalls[1] as PreviewCall)));
        assert.equal(seen2.length, 1);
        // The final drop forwards the preview prior and the cross-output source.
        assert.equal(adapter.requestDragDrop("win-a", 900, 5, "drag-20", { output: "out-9", workspace: "ws-9" }, "drag-20"), true);
        const dropCommand = commandOf(mocks.planCalls[mocks.planCalls.length - 1] as PreviewCall);
        assert.equal(dropCommand["op"], "drag-drop");
        assert.equal(dropCommand["source_output"], "out-9");
        assert.equal(dropCommand["source_workspace"], "ws-9");
        assert.deepEqual(dropCommand["hover_prior"], (seen[0] as PlanDragPreviewResult).hoverPrior);
        assertNoRawIds(mocks.logs, ["out-9", "ws-9"]);
        assert.ok(mocks.logs.some((l) => l.includes("drag-drop-cross-output") && l.includes("correlation=drag-20")));
        assert.equal(mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-20") && l.includes("dispatched=2")).length, 1);
        adapter.clearDragPreview("drag-20");
        assert.equal(mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-20")).length, 1, "drop consumption emits exactly one summary");
    });

    it("fences late replies and refusals; busy preview backs off without delaying the final drop", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        // Superseded sequence: the first reply is ignored entirely.
        const first: Array<PlanDragPreviewResult | null> = [];
        const second: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-10", undefined, (r) => first.push(r)), true);
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-10", undefined, (r) => second.push(r)), true);
        assert.notEqual(corrOf(mocks.previewCalls[0] as PreviewCall), corrOf(mocks.previewCalls[1] as PreviewCall));
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as PreviewCall)));
        assert.equal(first.length, 0, "superseded sequence is ignored");
        mocks.previewCalls[1]?.callback(previewReply(corrOf(mocks.previewCalls[1] as PreviewCall)));
        assert.equal(second.length, 1);
        assert.ok(second[0] !== null);
        // Destination scope change before the reply: stale, callback null.
        const scoped: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-11", undefined, (r) => scoped.push(r)), true);
        const scopedCorr = corrOf(mocks.previewCalls[2] as PreviewCall);
        mocks.observeImpl = (): PlanObserved | null => null;
        mocks.previewCalls[2]?.callback(previewReply(scopedCorr));
        assert.deepEqual(scoped, [null]);
        assert.ok(mocks.logs.some((l) => l.includes("drag-preview-settled") && l.includes("correlation=drag-11") && l.includes("stale")));
        // Refusal clears the carry: the next preview ships no hover_prior.
        mocks.observeImpl = () => makeObserved(refs);
        const refused: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-12", undefined, (r) => refused.push(r)), true);
        mocks.previewCalls[3]?.callback(rejectedReply(corrOf(mocks.previewCalls[3] as PreviewCall), "unsupported-capability"));
        assert.deepEqual(refused, [null]);
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-12", undefined, () => {}), false);
        assert.equal(mocks.previewCalls.length, 4, "identical refusal reuses instead of re-planning");
        const recarry: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-12", undefined, (r) => recarry.push(r)), true);
        assert.ok(!("hover_prior" in commandOf(mocks.previewCalls[4] as PreviewCall)), "refusal clears the carry");
        adapter.clearDragPreview();
        // Busy preview backs off without queueing.
        assert.equal(adapter.requestDragDrop("win-a", 900, 5, "drag-2"), true);
        assert.equal(adapter.isInFlight, true);
        const planCalls = mocks.planCalls.length;
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-3", undefined, () => {}), false);
        assert.equal(mocks.previewCalls.length, 5);
        assert.ok(mocks.logs.some((l) => l.includes("busy-refused") && l.includes("kind=drag-preview")));
        assert.ok(mocks.logs.some((l) => l.includes("drag-preview-backed-off") && l.includes("correlation=drag-3")));
        const dropCall = mocks.planCalls[mocks.planCalls.length - 1] as PreviewCall;
        dropCall.callback(JSON.stringify({
            v: 1, correlation_id: corrOf(dropCall), outcome: "planned", base_revision: 2,
            detail: { kind: "drag-drop", capability: "place-tiled" },
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 800, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 800, y: 0, w: 800, h: 800 } },
            ],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
        }));
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-4", undefined, () => {}), true);
        assert.equal(adapter.requestDragDrop("win-b", 100, 100, "drag-5"), true);
        assert.equal(mocks.planCalls.length, planCalls + 1);
        adapter.clearDragPreview();
    });

    it("cross-output pointer resolves in hidden destination with mover projection; outside refuses and stale dest fences", () => {
        const refs = makeRefs();
        const destRef: object = {};
        const mocks = mockEnv(refs);
        let hidden: PlanObserved[] = [makeDest(destRef)];
        (mocks.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = () => hidden;
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 1700, 100, "drag-30", { output: "out-1", workspace: "ws-1" }, (r) => seen.push(r)), true);
        const payload = JSON.parse(mocks.previewCalls[0]?.payload as string) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], "out-2");
        assert.equal(payload["focused_window"], "win-a");
        assert.equal(commandOf(mocks.previewCalls[0] as PreviewCall)["source_output"], "out-1");
        const windows = payload["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 2);
        assert.equal(windows.find((e) => e["window"] === "win-a")?.["output"], "out-2");
        assert.equal(windows.find((e) => e["window"] === "win-c")?.["output"], "out-2");
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as PreviewCall)));
        assert.ok(seen[0] !== null);
        const outsideSeen: Array<PlanDragPreviewResult | null> = [];
        const callsBefore = mocks.previewCalls.length;
        assert.equal(adapter.requestDragPreview("win-a", 5000, 5000, "drag-31", { output: "out-1", workspace: "ws-1" }, (r) => outsideSeen.push(r)), false);
        assert.equal(mocks.previewCalls.length, callsBefore);
        assert.deepEqual(outsideSeen, [null]);
        const staleSeen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 1700, 100, "drag-32", { output: "out-1", workspace: "ws-1" }, (r) => staleSeen.push(r)), true);
        const staleCorr = corrOf(mocks.previewCalls[mocks.previewCalls.length - 1] as PreviewCall);
        hidden = [];
        mocks.previewCalls[mocks.previewCalls.length - 1]?.callback(previewReply(staleCorr));
        assert.deepEqual(staleSeen, [null]);
        adapter.clearDragPreview();
    });

    it("ambiguous/unreadable destination refuses before transfer; preview callback reports null", () => {
        const refs = makeRefs();
        const destA = makeDest({});
        const mocks = mockEnv(refs);
        const holder: { hidden: () => PlanObserved[] } = { hidden: () => [destA, makeDest({})] };
        (mocks.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = () => holder.hidden();
        const adapter = enableAdapter(mocks);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 1700, 100, "drag-41", undefined, (r) => seen.push(r)), false);
        assert.deepEqual(seen, [null]);
        assert.equal(adapter.requestDragDrop("win-a", 1700, 100, "drag-42", { output: "out-1", workspace: "ws-1" }), false);
        assert.equal(commandOf(mocks.planCalls[0] as PreviewCall)["op"], "reconcile");
        const mocksUnread = mockEnv(refs);
        (mocksUnread.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = (): PlanObserved[] => {
            throw new Error("unreadable");
        };
        const adapterUnread = enableAdapter(mocksUnread);
        const seenUnread: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapterUnread.requestDragPreview("win-a", 1700, 100, "drag-43", undefined, (r) => seenUnread.push(r)), false);
        assert.deepEqual(seenUnread, [null]);
        assert.equal(mocksUnread.previewCalls.length, 0);
        assertNoRawIds(mocks.logs, ["out-2", "out-3", "win-a"]);
        assert.ok(mocks.logs.some((l) => l.includes("drag-drop-refused-outside")));
    });

    it("Finish-lag drop binds the destination snapshot and applies transfer, membership, geometry in order", () => {
        const refs = makeRefs();
        const destRef: object = {};
        const mocks = mockEnv(refs);
        let outputOfMover = "out-1";
        let desktopsOfMover = ["ws-1"];
        (mocks.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = () =>
            outputOfMover === "out-2" ? [makeDest(destRef, {}, undefined, refs.a)] : [makeDest(destRef)];
        const extra = mocks.env as unknown as Record<string, unknown>;
        const events = installTransfer(mocks);
        extra["readWindowConstraints"] = undefined;
        extra["sendClientToScreen"] = (mover: object, output: object): boolean => {
            events.push("transfer");
            if (mover === refs.a && (output as { name: string }).name === "out-2") {
                outputOfMover = "out-2";
            }
            return mover === refs.a;
        };
        extra["setDesktops"] = (mover: object, list: ReadonlyArray<object>): boolean => {
            events.push("membership");
            if (mover === refs.a && list.length === 1) {
                desktopsOfMover = ["ws-1"];
            }
            return mover === refs.a && list.length === 1;
        };
        extra["readOutputName"] = (ref: object): string | null => (ref === refs.a ? outputOfMover : null);
        extra["readDesktopIds"] = (ref: object): ReadonlyArray<string> | null => (ref === refs.a ? [...desktopsOfMover] : null);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestDragDrop("win-a", 1700, 100, "drag-50", { output: "out-1", workspace: "ws-1" }), true);
        const dropCall = mocks.planCalls[0] as PreviewCall;
        const payload = JSON.parse(dropCall.payload) as Record<string, unknown>;
        assert.equal((payload["domain"] as Record<string, unknown>)["output"], "out-2");
        assert.equal(payload["focused_window"], "win-a");
        assert.equal(commandOf(dropCall)["source_output"], "out-1");
        assert.equal((payload["windows"] as Array<Record<string, unknown>>).find((e) => e["window"] === "win-a")?.["output"], "out-2");
        const dropCorr = corrOf(dropCall);
        dropCall.callback(plannedCrossReply(dropCorr));
        assert.deepEqual(events.filter((e) => e === "transfer" || e === "membership" || e === "geometry").slice(0, 2), ["transfer", "membership"]);
        assert.ok(events.filter((e) => e === "geometry").length >= 2, "geometries apply after native transfer");
        assert.ok(mocks.logs.some((l) => l.includes("drag-drop-cross-applied") && l.includes("correlation=drag-50")));
        assert.ok(mocks.logs.some((l) => l.includes(`cmd=${dropCorr}`) && l.includes("outcome=planned-applied")));
        assertNoRawIds(mocks.logs, ["out-1", "out-2", "ws-1"]);
    });

    it("Finish-lag arrival exactness: without observed destination membership nothing applies", () => {
        const cases: Array<{ drag: string; readOutput: string | null; readDesktops: ReadonlyArray<string> | null; withCapability: boolean }> = [
            { drag: "drag-55", readOutput: "out-2", readDesktops: ["ws-1"], withCapability: true },
            { drag: "drag-53", readOutput: "out-1", readDesktops: ["ws-1"], withCapability: true },
            { drag: "drag-57", readOutput: "out-2", readDesktops: ["ws-9"], withCapability: true },
            { drag: "drag-51", readOutput: "out-2", readDesktops: ["ws-1"], withCapability: false },
        ];
        for (const kase of cases) {
            const refs = makeRefs();
            const destRef: object = {};
            const mocks = mockEnv(refs);
            (mocks.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = () => [makeDest(destRef)];
            const geometries = { count: 0 };
            if (kase.withCapability) {
                installTransfer(mocks, { readOutput: kase.readOutput, readDesktops: kase.readDesktops, countGeometries: geometries });
            }
            const adapter = enableAdapter(mocks);
            assert.equal(adapter.requestDragDrop("win-a", 1700, 100, kase.drag, { output: "out-1", workspace: "ws-1" }), true);
            const dropCall = mocks.planCalls[0] as PreviewCall;
            const dropCorr = corrOf(dropCall);
            dropCall.callback(plannedCrossReply(dropCorr));
            assert.equal(geometries.count, 0, `${kase.drag}: no geometry without exact arrival`);
            assert.ok(!mocks.logs.some((l) => l.includes(`cmd=${dropCorr}`) && l.includes("outcome=planned-applied")), `${kase.drag}: never claims applied`);
            assert.ok(mocks.logs.some((l) => l.includes("drag-drop-cross-refused") && l.includes(`correlation=${kase.drag}`)), `${kase.drag}: correlated refusal`);
        }
    });

    it("Finish-lag survivor drift or flag flip fails before any geometry applies", () => {
        const prewrite: Array<{ drag: string; rect?: Rect; extra?: Partial<PlanObservedWindow>; postFlip?: "fullscreen" | "maximized" }> = [
            { drag: "drag-52", rect: { x: 1610, y: 0, w: 600, h: 800 } },
            { drag: "drag-56", extra: { floating: true } },
            { drag: "drag-58", extra: { fullscreen: true } },
            { drag: "drag-59", extra: { maximized: true } },
            { drag: "drag-60", postFlip: "fullscreen" },
            { drag: "drag-61", postFlip: "maximized" },
        ];
        for (const kase of prewrite) {
            const refs = makeRefs();
            const destRef: object = {};
            const mocks = mockEnv(refs);
            let extra: Partial<PlanObservedWindow> = {};
            let rect: Rect = { x: 1600, y: 0, w: 600, h: 800 };
            let outputOfMover = "out-1";
            (mocks.env as unknown as { observeHidden?: () => PlanObserved[] }).observeHidden = () => [
                makeDest(
                    destRef,
                    kase.postFlip !== undefined && outputOfMover === "out-2" ? { [kase.postFlip]: true } as Partial<PlanObservedWindow> : extra,
                    rect,
                    kase.postFlip !== undefined && outputOfMover === "out-2" ? refs.a : null,
                ),
            ];
            let transfers = 0;
            const geometries = { count: 0 };
            installTransfer(mocks, {
                countGeometries: geometries,
                onTransfer: () => {
                    transfers += 1;
                    if (kase.postFlip) {
                        outputOfMover = "out-2";
                    }
                },
            });
            const adapter = enableAdapter(mocks);
            assert.equal(adapter.requestDragDrop("win-a", 1700, 100, kase.drag, { output: "out-1", workspace: "ws-1" }), true);
            const dropCall = mocks.planCalls[0] as PreviewCall;
            const dropCorr = corrOf(dropCall);
            extra = kase.extra ?? {};
            rect = kase.rect ?? rect;
            dropCall.callback(plannedCrossReply(dropCorr));
            if (kase.postFlip === undefined) {
                assert.equal(transfers, 0, `${kase.drag} blocks transfer`);
            }
            assert.equal(geometries.count, 0, `${kase.drag}: no geometry on survivor drift`);
            assert.ok(!mocks.logs.some((l) => l.includes(`cmd=${dropCorr}`) && l.includes("outcome=planned-applied")), `${kase.drag}: no applied claim`);
        }
    });
});

describe("drag-preview per-sample dedup and state-change logging", () => {
    function appliedAt(mocks: Mocks, adapter: PlanAdapter, drag: string, x: number, y: number): PlanDragPreviewResult {
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", x, y, drag, undefined, (r) => seen.push(r)), true);
        const call = mocks.previewCalls[mocks.previewCalls.length - 1] as PreviewCall;
        call.callback(previewReply(corrOf(call)));
        assert.equal(seen.length, 1);
        assert.ok(seen[0] !== null);
        return seen[0] as PlanDragPreviewResult;
    }
    function settledCount(mocks: Mocks, drag: string): number {
        return mocks.logs.filter((l) => l.includes("drag-preview-settled") && l.includes(`correlation=${drag}`)).length;
    }

    it("reuses the cached result without a new plan for an unchanged pointer once the prior is stable", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        appliedAt(mocks, adapter, "drag-70", 900, 5);
        assert.equal(mocks.previewCalls.length, 1);
        // Same pointer, but the validated prior now rides: exactly one re-plan.
        appliedAt(mocks, adapter, "drag-70", 900, 5);
        assert.equal(mocks.previewCalls.length, 2, "prior evolution re-plans once");
        const settledAfterTwo = settledCount(mocks, "drag-70");
        // Same pointer, same scene, same prior: reuse, no new D-Bus call.
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-70", undefined, (r) => seen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 2, "unchanged sample reuses instead of re-planning");
        assert.equal(seen.length, 1, "reuse still answers the caller");
        assert.deepEqual(seen[0]?.rect, { x: 600, y: 0, w: 600, h: 800 });
        assert.equal(settledCount(mocks, "drag-70"), settledAfterTwo, "identical result logs no second settled line");
        adapter.clearDragPreview("drag-70");
    });

    it("re-plans when the pointer moves to a new edge", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        appliedAt(mocks, adapter, "drag-71", 900, 5);
        appliedAt(mocks, adapter, "drag-71", 900, 5);
        assert.equal(mocks.previewCalls.length, 2);
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-71", undefined, (r) => seen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 3, "moved pointer re-plans");
        adapter.clearDragPreview("drag-71");
    });

    it("re-plans on the same pointer after topology or work-area change, but not on mover rect drift", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        appliedAt(mocks, adapter, "drag-72", 900, 5);
        appliedAt(mocks, adapter, "drag-72", 900, 5);
        assert.equal(mocks.previewCalls.length, 2);
        // Native drag geometry drift on the mover alone never re-plans.
        const drifted = makeObserved(refs);
        mocks.observeImpl = (): PlanObserved | null => ({
            ...drifted,
            windows: Object.freeze([
                Object.freeze(tile("win-a", refs.a, "out-1", { x: 300, y: 200, w: 600, h: 800 })),
                Object.freeze(tile("win-b", refs.b, "out-1", { x: 600, y: 0, w: 600, h: 800 })),
            ]),
        });
        const driftedSeen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-72", undefined, (r) => driftedSeen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 2, "mover rect drift alone reuses");
        assert.equal(driftedSeen.length, 1);
        // A membership change on the same pointer re-plans.
        mocks.observeImpl = (): PlanObserved | null => {
            const base = makeObserved(refs);
            return {
                ...base,
                windows: Object.freeze([...base.windows, Object.freeze(tile("win-c", {}, "out-1", { x: 1200, y: 0, w: 400, h: 800 }))]),
            };
        };
        const topoSeen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-72", undefined, (r) => topoSeen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 3, "topology change re-plans on the same pointer");
        // A work-area change on the same pointer re-plans.
        mocks.observeImpl = (): PlanObserved | null => ({ ...makeObserved(refs), domainBounds: { x: 0, y: 0, w: 1920, h: 1080 } });
        const boundsSeen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-72", undefined, (r) => boundsSeen.push(r)), true);
        assert.equal(mocks.previewCalls.length, 4, "work-area change re-plans on the same pointer");
        adapter.clearDragPreview("drag-72");
    });

    it("identical settled results log once; a refused transition still logs with its cause", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        appliedAt(mocks, adapter, "drag-73", 900, 5);
        assert.equal(settledCount(mocks, "drag-73"), 1);
        appliedAt(mocks, adapter, "drag-73", 900, 5);
        assert.equal(settledCount(mocks, "drag-73"), 1, "identical applied result logs once");
        const refused: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 5000, 5000, "drag-73", undefined, (r) => refused.push(r)), false);
        assert.deepEqual(refused, [null]);
        assert.ok(
            mocks.logs.some((l) => l.includes("drag-preview-refused") && l.includes("correlation=drag-73")),
            "refusal transition keeps its cause",
        );
        adapter.clearDragPreview("drag-73");
    });

    it("ephemeral planner refusals re-dispatch on the same pointer and stay bounded", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const first: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-74", undefined, (r) => first.push(r)), true);
        assert.equal(mocks.previewCalls.length, 1);
        mocks.previewCalls[0]?.callback(rejectedReply(corrOf(mocks.previewCalls[0] as PreviewCall), "not-tiled"));
        assert.deepEqual(first, [null]);
        assert.equal(settledCount(mocks, "drag-74"), 1);
        // An ephemeral refusal never caches: the identical pointer
        // re-plans so it recovers when planner/session state changes.
        for (let repeat = 0; repeat < 3; repeat += 1) {
            const seen: Array<PlanDragPreviewResult | null> = [];
            assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-74", undefined, (r) => seen.push(r)), true);
            assert.deepEqual(seen, []);
            mocks.previewCalls[mocks.previewCalls.length - 1]?.callback(
                rejectedReply(corrOf(mocks.previewCalls[mocks.previewCalls.length - 1] as PreviewCall), "not-tiled"),
            );
            assert.deepEqual(seen, [null]);
        }
        assert.equal(mocks.previewCalls.length, 4, "ephemeral refusal re-dispatches instead of reusing");
        assert.equal(settledCount(mocks, "drag-74"), 1, "identical refusal logs once");
        const moved: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-74", undefined, (r) => moved.push(r)), true);
        assert.equal(mocks.previewCalls.length, 5, "moved pointer re-plans after a refusal");
        adapter.clearDragPreview("drag-74");
        assert.ok(
            mocks.logs.some((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-74")),
            "refused drag still ends with a bounded summary",
        );
    });

    it("ephemeral cross-domain and topology refusals recover on the same pointer; deterministic ones reuse", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const refuseKind = (kind: string): Array<PlanDragPreviewResult | null> => {
            const seen: Array<PlanDragPreviewResult | null> = [];
            assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-80", undefined, (r) => seen.push(r)), true);
            mocks.previewCalls[mocks.previewCalls.length - 1]?.callback(
                rejectedReply(corrOf(mocks.previewCalls[mocks.previewCalls.length - 1] as PreviewCall), kind),
            );
            return seen;
        };
        assert.deepEqual(refuseKind("cross-domain-mismatch"), [null]);
        assert.equal(mocks.previewCalls.length, 1);
        // Same pointer after an ephemeral refusal re-plans (no indefinite cache).
        const retry: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-80", undefined, (r) => retry.push(r)), true);
        assert.equal(mocks.previewCalls.length, 2, "cross-domain-mismatch re-dispatches on the same pointer");
        mocks.previewCalls[1]?.callback(rejectedReply(corrOf(mocks.previewCalls[1] as PreviewCall), "malformed-topology"));
        assert.deepEqual(retry, [null]);
        const retryTopo: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-80", undefined, (r) => retryTopo.push(r)), true);
        assert.equal(mocks.previewCalls.length, 3, "malformed-topology re-dispatches on the same pointer");
        mocks.previewCalls[2]?.callback(rejectedReply(corrOf(mocks.previewCalls[2] as PreviewCall), "unsupported-capability"));
        assert.deepEqual(retryTopo, [null]);
        // Deterministic refusals still reuse without re-planning.
        const cached: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-80", undefined, (r) => cached.push(r)), false);
        assert.deepEqual(cached, [null]);
        assert.equal(mocks.previewCalls.length, 3, "unsupported-capability reuses instead of re-planning");
        // A moved pointer always re-plans.
        const moved: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 901, 6, "drag-80", undefined, (r) => moved.push(r)), true);
        assert.equal(mocks.previewCalls.length, 4, "moved pointer re-plans");
        adapter.clearDragPreview("drag-80");
        assert.ok(
            mocks.logs.some((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-80")),
            "refused drag still ends with a bounded summary",
        );
    });

    it("same pointer re-plans after a committed planner state change under identical ids and bounds", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const geometries: Array<{ x: number; y: number; w: number; h: number }> = [];
        const baseSetGeometry = mocks.env.setGeometry;
        (mocks.env as unknown as Record<string, unknown>)["setGeometry"] = (target: object, rect: { x: number; y: number; w: number; h: number }): boolean => {
            geometries.push({ ...rect });
            return baseSetGeometry(target, rect);
        };
        const adapter = enableAdapter(mocks);
        // Cache one applied preview on drag-90: the first reply sets the
        // carried prior (one evolution re-plan), the next identical sample
        // reuses.
        const seen: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-90", undefined, (r) => seen.push(r)), true);
        mocks.previewCalls[0]?.callback(previewReply(corrOf(mocks.previewCalls[0] as PreviewCall)));
        assert.ok(seen[0] !== null);
        const evolved: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-90", undefined, (r) => evolved.push(r)), true);
        assert.equal(mocks.previewCalls.length, 2, "prior evolution re-plans once");
        mocks.previewCalls[1]?.callback(previewReply(corrOf(mocks.previewCalls[1] as PreviewCall)));
        const stable: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-90", undefined, (r) => stable.push(r)), true);
        assert.equal(mocks.previewCalls.length, 2, "third identical sample reuses before any commit");
        // Commit planner state with a layout command that keeps every
        // window id, bound, gap, and flag identical while changing the
        // retained tree (axis flip).
        adapter.requestToggleOrientation();
        assert.equal(mocks.planCalls.length, 1, "orientation dispatches through the single flight");
        const orientCorr = corrOf(mocks.planCalls[0] as PreviewCall);
        mocks.planCalls[0]?.callback(JSON.stringify({
            v: 1, correlation_id: orientCorr, outcome: "planned", base_revision: 2,
            detail: { kind: "toggle-orientation", policy_version: 1, capability: "toggle-orientation" },
            desired_geometry: [
                { window: "win-a", leaf: "win-a-leaf", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 800, h: 800 } },
                { window: "win-b", leaf: "win-b-leaf", output: "out-1", workspace: "ws-1", rect: { x: 800, y: 0, w: 400, h: 800 } },
            ],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "win-a-leaf" },
        }));
        assert.ok(geometries.length >= 2, "orientation commits geometry");
        assert.ok(
            mocks.logs.some((l) => l.includes(`cmd=${orientCorr}`) && l.includes("outcome=planned-applied")),
            "orientation commits planner state",
        );
        // Identical ids, bounds, flags, and prior on drag-90 must re-plan:
        // the retained tree changed under the cache.
        const after: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-90", undefined, (r) => after.push(r)), true);
        assert.equal(mocks.previewCalls.length, 3, "committed state change invalidates the preview cache");
        adapter.clearDragPreview("drag-90");
    });

    it("adapter-side refuse and finish-edge fence preserve the end summary", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        // One dispatched preview, then an adapter-side outside refuse.
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-81", undefined, () => {}), true);
        assert.equal(mocks.previewCalls.length, 1);
        const refused: Array<PlanDragPreviewResult | null> = [];
        assert.equal(adapter.requestDragPreview("win-a", 5000, 5000, "drag-81", undefined, (r) => refused.push(r)), false);
        assert.deepEqual(refused, [null]);
        assert.ok(
            mocks.logs.some((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-81") && l.includes("dispatched=1")),
            "adapter-side refuse keeps the dispatched count in the end summary",
        );
        const summaries81 = mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-81"));
        adapter.clearDragPreview("drag-81");
        assert.equal(
            mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-81")).length,
            summaries81.length,
            "clearing after a refuse emits no second summary",
        );
        // Finish-edge fence with no applied prior yet still ends summarized.
        assert.equal(adapter.requestDragPreview("win-a", 900, 5, "drag-82", undefined, () => {}), true);
        adapter.invalidateDragPreviewReply("drag-82");
        assert.ok(
            mocks.logs.some((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-82") && l.includes("dispatched=1")),
            "finish-edge fence keeps the dispatched count in the end summary",
        );
        const summaries82 = mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-82"));
        adapter.clearDragPreview("drag-82");
        assert.equal(
            mocks.logs.filter((l) => l.includes("drag-preview-summary") && l.includes("correlation=drag-82")).length,
            summaries82.length,
            "clearing after the fence emits no second summary",
        );
    });
});
