// D23 close-versus-minimize distinction through the production PlanAdapter
// with a mocked transport (no planner binary).
//
// - Reconcile requests omit `closed_windows` while no close is pending
//   (byte-identical wire shape).
// - A verified close rides the next reconcile as `closed_windows` and clears
//   only on planner delivery (`planned`); a `rejected` reply retains it and
//   the next resync redelivers.
// - A close for a live-observed id never dispatches (contradictory rows stay
//   fail-closed planner-side; the stale entry drops locally).
// - A null foreground observation with a pending close on a retained domain
//   still delivers one synthesized empty-domain cleanup (sole-minimize close).

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    PLAN_DEBOUNCE_MS,
    PLAN_METHOD,
    PlanAdapter,
    PlanAdapterEnv,
    PlanObserved,
} from "../src/plan-adapter";
import { observeHiddenDomains } from "../src/plan-adapter-entry";
import { DOMAIN_GAP, OUTER_DOMAIN_GAP, type DomainGaps } from "../src/domain-gap";

function makeRefs(): { a: object; b: object } {
    return { a: {}, b: {} };
}

function win(id: string, ref: object, rect: { x: number; y: number; w: number; h: number }): Record<string, unknown> {
    return {
        id,
        ref,
        rect,
        output: "out-1",
        workspace: "ws-1",
        fullscreen: false,
        maximized: false,
        floating: false,
        sticky: false,
        resourceClass: "unknown",
    };
}

function makeObserved(
    refs: { a: object; b: object },
    ids: string[],
    focused: string,
): PlanObserved {
    const rects: Record<string, { x: number; y: number; w: number; h: number }> = {
        "win-a": { x: 0, y: 0, w: 100, h: 100 },
        "win-b": { x: 200, y: 0, w: 100, h: 100 },
    };
    const refOf = (id: string): object => (id === "win-a" ? refs.a : refs.b);
    const windows = Object.freeze(
        ids.map((id) => Object.freeze(win(id, refOf(id), rects[id] as { x: number; y: number; w: number; h: number }))),
    );
    const activeRef = ids.length === 0 ? refs.a : focused === "win-a" ? refs.a : refs.b;
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-1",
        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: DOMAIN_GAP,
        domainOuterGap: OUTER_DOMAIN_GAP,
        focusedId: focused,
        ...(ids.length === 0 ? { activeExcluded: true as const } : {}),
        windows: windows as unknown as PlanObserved["windows"],
        activeRef,
        fingerprint: `fp-${ids.join("+")}-${focused}`,
        revalidate: () => true,
    };
}

interface Mocks {
    readonly dbusCalls: Array<{ service: string; path: string; iface: string; method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    observeImpl: () => PlanObserved | null;
    hiddenImpl: () => ReadonlyArray<PlanObserved>;
    env: PlanAdapterEnv;
}

function mockEnv(refs: { a: object; b: object }): Mocks {
    const state: Mocks = {
        dbusCalls: [],
        callbacks: [],
        timers: [],
        logs: [],
        observeImpl: () => makeObserved(refs, ["win-a", "win-b"], "win-a"),
        hiddenImpl: () => [],
        env: null as unknown as PlanAdapterEnv,
    };
    state.env = {
        callDbus: (service, path, iface, method, payload, callback): void => {
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
        observeHidden: (): ReadonlyArray<PlanObserved> => state.hiddenImpl(),
        clearMaximize: (): "invoked" => "invoked",
        setGeometry: (): boolean => true,
        setActive: (): boolean => true,
        active: () => refs.a,
        subscribe: (): (() => void) => (): void => {},
    };
    return state;
}

function enableAdapter(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function fireDebounce(mocks: Mocks): void {
    for (const timer of mocks.timers) {
        if (timer.delayMs === PLAN_DEBOUNCE_MS && !timer.cancelled) {
            timer.cancelled = true;
            timer.callback();
        }
    }
}

function planIndex(mocks: Mocks, from: number): number {
    for (let index = from; index < mocks.dbusCalls.length; index += 1) {
        if (mocks.dbusCalls[index]?.method === PLAN_METHOD) {
            return index;
        }
    }
    throw new Error("no DescribePlan call dispatched");
}

// Drive one debounced reconcile round trip: presence, owner pin, then the
// DescribePlan callback with `replyJson`. Returns the DescribePlan payload.
function roundTrip(mocks: Mocks, replyJson: (correlation: string) => string): Record<string, unknown> {
    mocks.dbusCalls.length = 0;
    mocks.callbacks.length = 0;
    fireDebounce(mocks);
    assert.ok(mocks.dbusCalls.length >= 1, `expected presence call, got ${mocks.dbusCalls.length}`);
    assert.equal(mocks.dbusCalls[0]?.method, "NameHasOwner");
    mocks.callbacks[0]?.(true);
    assert.equal(mocks.dbusCalls[1]?.method, "GetNameOwner");
    mocks.callbacks[1]?.(":1.5");
    const at = planIndex(mocks, 0);
    const payload = JSON.parse(mocks.dbusCalls[at]?.payload as string) as Record<string, unknown>;
    const correlation = payload["correlation_id"] as string;
    mocks.callbacks[at]?.(replyJson(correlation));
    return payload;
}

function plannedFor(ids: string[]): (correlation: string) => string {
    return (correlation: string): string =>
        JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "planned",
            desired_geometry: ids.map((id) => ({
                window: id,
                leaf: `${id}-leaf`,
                output: "out-1",
                workspace: "ws-1",
                rect: id === "win-a" ? { x: 0, y: 0, w: 100, h: 100 } : { x: 200, y: 0, w: 100, h: 100 },
            })),
        });
}

function rejected(correlation: string): string {
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "rejected",
        kind: "partial-observation",
        message: "observation does not cover the known window set",
    });
}

function hiddenRefs(): Record<string, object> {
    return { "win-x": {}, "win-y": {} };
}

function makeHidden(
    output: string,
    workspace: string,
    ids: string[],
    refs: Record<string, object>,
): PlanObserved {
    const windows = Object.freeze(
        ids.map((id) =>
            Object.freeze({
                id,
                ref: refs[id] as object,
                rect: { x: 0, y: 0, w: 100, h: 100 },
                output,
                workspace,
                fullscreen: false,
                maximized: false,
                floating: false,
                sticky: false,
                resourceClass: "unknown",
            }),
        ),
    );
    return {
        domainOutput: output,
        domainWorkspace: workspace,
        domainBounds: { x: 1920, y: 0, w: 1200, h: 800 },
        domainGap: DOMAIN_GAP,
        domainOuterGap: OUTER_DOMAIN_GAP,
        focusedId: ids.length === 0 ? "" : (ids[0] as string),
        ...(ids.length === 0 ? { activeExcluded: true as const } : {}),
        windows: windows as unknown as PlanObserved["windows"],
        activeRef: {},
        fingerprint: `hid-${output}-${workspace}-${ids.join("+")}`,
        revalidate: () => true,
    };
}

function plannedHidden(output: string, workspace: string, ids: string[]): (correlation: string) => string {
    return (correlation: string): string =>
        JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "planned",
            desired_geometry: ids.map((id) => ({
                window: id,
                leaf: `${id}-leaf`,
                output,
                workspace,
                rect: { x: 0, y: 0, w: 100, h: 100 },
            })),
        });
}

describe("plan close-versus-minimize distinction", () => {
    it("omits closed_windows while no close is pending", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            adapter.requestResync();
            const payload = roundTrip(mocks, plannedFor(["win-a", "win-b"]));
            assert.ok(!("closed_windows" in payload), "no close pending means no wire field");
            assert.deepEqual(adapter.getPendingClosed(), []);
        } finally {
            adapter.disable();
        }
    });

    it("close rides the next reconcile and clears on planned", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            // Baseline: both tiles converge.
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a", "win-b"]));
            // Minimize B: survivors converge without the close.
            mocks.observeImpl = () => makeObserved(refs, ["win-a"], "win-a");
            adapter.requestResync();
            const minPayload = roundTrip(mocks, plannedFor(["win-a"]));
            assert.ok(!("closed_windows" in minPayload), "minimize omission carries no close");
            // Verified close of minimized B queues the cleanup.
            adapter.noteClosedWindow("win-b", "out-1", "ws-1");
            assert.deepEqual(adapter.getPendingClosed(), ["win-b"]);
            const closePayload = roundTrip(mocks, plannedFor(["win-a"]));
            assert.deepEqual(closePayload["closed_windows"], ["win-b"], "close rides the forced reconcile");
            assert.deepEqual(
                (closePayload["windows"] as Array<Record<string, unknown>>).map((e) => e["window"]),
                ["win-a"],
                "survivors still converge on the close turn",
            );
            assert.deepEqual(adapter.getPendingClosed(), [], "planned delivery clears the close");
            // The next round trip stays quiet and carries no close field.
            mocks.dbusCalls.length = 0;
            mocks.callbacks.length = 0;
            adapter.requestResync();
            fireDebounce(mocks);
            assert.ok(
                !mocks.dbusCalls.some((c) => c.method === PLAN_METHOD),
                "converged close stays quiet afterwards",
            );
        } finally {
            adapter.disable();
        }
    });

    it("rejected close retains and redelivers on the next resync", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a", "win-b"]));
            mocks.observeImpl = () => makeObserved(refs, ["win-a"], "win-a");
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a"]));
            adapter.noteClosedWindow("win-b", "out-1", "ws-1");
            const first = roundTrip(mocks, rejected);
            assert.deepEqual(first["closed_windows"], ["win-b"], "rejected flight carried the close");
            assert.deepEqual(adapter.getPendingClosed(), ["win-b"], "rejected close is retained");
            // Next signal redelivers the retained close.
            adapter.requestResync();
            const second = roundTrip(mocks, plannedFor(["win-a"]));
            assert.deepEqual(second["closed_windows"], ["win-b"], "retained close redelivers");
            assert.deepEqual(adapter.getPendingClosed(), [], "redelivery clears on planned");
        } finally {
            adapter.disable();
        }
    });

    it("live-observed close drops locally and never dispatches", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a", "win-b"]));
            // The id is still live: the close is stale (id reuse) and drops,
            // so the converged observation stays quiet with no dispatch.
            adapter.noteClosedWindow("win-a", "out-1", "ws-1");
            assert.deepEqual(adapter.getPendingClosed(), ["win-a"], "close queued before the refresh");
            mocks.dbusCalls.length = 0;
            mocks.callbacks.length = 0;
            adapter.requestResync();
            fireDebounce(mocks);
            assert.ok(
                !mocks.dbusCalls.some((c) => c.method === PLAN_METHOD),
                "stale close drops without dispatching",
            );
            assert.deepEqual(adapter.getPendingClosed(), [], "stale close drops");
        } finally {
            adapter.disable();
        }
    });

    it("null observation still delivers a sole-close cleanup", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            // Sole tile converges (retains the dispatched domain shape).
            mocks.observeImpl = () => makeObserved(refs, ["win-a"], "win-a");
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a"]));
            // Sole minimize converges explicit empty evidence.
            mocks.observeImpl = () => makeObserved(refs, [], "");
            adapter.requestResync();
            const minPayload = roundTrip(mocks, plannedFor([]));
            assert.deepEqual(minPayload["windows"], [], "sole minimize carries explicit empty evidence");
            // The sole minimized window closes: no live observation remains.
            adapter.noteClosedWindow("win-a", "out-1", "ws-1");
            mocks.observeImpl = () => null;
            adapter.requestResync();
            mocks.dbusCalls.length = 0;
            mocks.callbacks.length = 0;
            fireDebounce(mocks);
            assert.ok(mocks.dbusCalls.length >= 1, "cleanup presence call dispatched");
            mocks.callbacks[0]?.(true);
            mocks.callbacks[1]?.(":1.5");
            const at = planIndex(mocks, 0);
            const payload = JSON.parse(mocks.dbusCalls[at]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(payload["windows"], [], "cleanup carries empty evidence");
            assert.deepEqual(payload["closed_windows"], ["win-a"], "cleanup carries the close");
            const correlation = payload["correlation_id"] as string;
            mocks.callbacks[0]?.(true);
            mocks.callbacks[1]?.(":1.5");
            mocks.callbacks[at]?.(
                JSON.stringify({ v: 1, correlation_id: correlation, outcome: "planned", desired_geometry: [] }),
            );
            assert.deepEqual(adapter.getPendingClosed(), [], "sole-close delivery clears");
        } finally {
            adapter.disable();
        }
    });

    it("throwing observation with live members dispatches nothing and retains the close", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        try {
            // Converged pair: the planner holds both members, so no
            // accepted-empty proof exists for the domain.
            mocks.observeImpl = () => makeObserved(refs, ["win-a", "win-b"], "win-a");
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a", "win-b"]));
            // A close queues while the observer goes unreadable (throwing)
            // with both members still alive.
            adapter.noteClosedWindow("win-x", "out-1", "ws-1");
            mocks.observeImpl = () => {
                throw new Error("unreadable");
            };
            mocks.dbusCalls.length = 0;
            mocks.callbacks.length = 0;
            adapter.requestResync();
            fireDebounce(mocks);
            assert.ok(
                !mocks.dbusCalls.some((c) => c.method === PLAN_METHOD),
                "unknown must never upgrade to fabricated empty evidence",
            );
            assert.deepEqual(adapter.getPendingClosed(), ["win-x"], "close retained for the next readable signal");
        } finally {
            adapter.disable();
        }
    });

    it("hidden minimized close forces a hidden reconcile carrying the close", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const hrefs = hiddenRefs();
        const HB = "out-2";
        const WB = "ws-2";
        try {
            // Foreground converges first (it always wins the round), then
            // the hidden domain converges with x.
            mocks.observeImpl = () => makeObserved(refs, ["win-a"], "win-a");
            mocks.hiddenImpl = () => [];
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a"]));
            mocks.hiddenImpl = () => [makeHidden(HB, WB, ["win-x"], hrefs)];
            adapter.requestResync();
            const seed = roundTrip(mocks, plannedHidden(HB, WB, ["win-x"]));
            assert.equal((seed["domain"] as Record<string, unknown>)["output"], HB);
            // Hidden minimize: verified-empty evidence converges, then the
            // close arrives while the observation stays converged-empty.
            mocks.hiddenImpl = () => [makeHidden(HB, WB, [], hrefs)];
            adapter.requestResync();
            roundTrip(mocks, plannedHidden(HB, WB, []));
            adapter.noteClosedWindow("win-x", HB, WB);
            const cleanup = roundTrip(mocks, plannedHidden(HB, WB, []));
            assert.deepEqual(cleanup["windows"], [], "hidden cleanup carries empty evidence");
            assert.deepEqual(cleanup["closed_windows"], ["win-x"], "hidden close rides the forced reconcile");
            assert.deepEqual(adapter.getPendingClosed(), [], "hidden close clears on planned");
        } finally {
            adapter.disable();
        }
    });

    it("simultaneous multi-domain closes drain without new signals", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const hrefs = hiddenRefs();
        try {
            mocks.observeImpl = () => makeObserved(refs, ["win-a"], "win-a");
            adapter.requestResync();
            roundTrip(mocks, plannedFor(["win-a"]));
            // Two hidden domains, both verified-empty, both with a close.
            mocks.hiddenImpl = () => [makeHidden("out-2", "ws-2", [], hrefs), makeHidden("out-3", "ws-3", [], hrefs)];
            adapter.noteClosedWindow("win-x", "out-2", "ws-2");
            adapter.noteClosedWindow("win-y", "out-3", "ws-3");
            mocks.dbusCalls.length = 0;
            mocks.callbacks.length = 0;
            adapter.requestResync();
            fireDebounce(mocks);
            // First domain dispatches.
            assert.equal(mocks.dbusCalls[0]?.method, "NameHasOwner");
            mocks.callbacks[0]?.(true);
            mocks.callbacks[1]?.(":1.5");
            const firstAt = planIndex(mocks, 0);
            const first = JSON.parse(mocks.dbusCalls[firstAt]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(first["closed_windows"], ["win-x"], "first hidden close dispatches");
            // Driving its reply drains the second domain with no new resync:
            // the chained dispatch starts its own activation, which the
            // test drives stepwise like any other flight.
            mocks.callbacks[firstAt]?.(plannedHidden("out-2", "ws-2", [])(first["correlation_id"] as string));
            let presenceAt = -1;
            for (let i = firstAt + 1; i < mocks.dbusCalls.length; i += 1) {
                if (mocks.dbusCalls[i]?.method === "NameHasOwner") {
                    presenceAt = i;
                    break;
                }
            }
            assert.ok(presenceAt >= 0, "chained dispatch starts without a new resync");
            assert.equal(mocks.dbusCalls[presenceAt]?.method, "NameHasOwner");
            mocks.callbacks[presenceAt]?.(true);
            assert.equal(mocks.dbusCalls[presenceAt + 1]?.method, "GetNameOwner");
            mocks.callbacks[presenceAt + 1]?.(":1.5");
            const secondAt = planIndex(mocks, presenceAt + 2);
            const second = JSON.parse(mocks.dbusCalls[secondAt]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(second["closed_windows"], ["win-y"], "second hidden close drains via the chain");
            mocks.callbacks[secondAt]?.(plannedHidden("out-3", "ws-3", [])(second["correlation_id"] as string));
            assert.deepEqual(adapter.getPendingClosed(), [], "all closes drain");
        } finally {
            adapter.disable();
        }
    });

    it("observeHiddenDomains records homing for hidden and minimized members", () => {
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
        const out2 = { name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } };
        const ws1 = { id: "ws-1", x11DesktopNumber: 1 };
        const ws2 = { id: "ws-2", x11DesktopNumber: 2 };
        const frame = { x: 0, y: 0, width: 400, height: 300 };
        const mkWin = (output: object, desks: object[], id: string, minimized: boolean): Record<string, unknown> => ({
            normalWindow: true,
            managed: true,
            minimized,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            output,
            desktops: desks,
            internalId: id,
            frameGeometry: { ...frame },
            resourceClass: "app",
        });
        const winA = mkWin(out1, [ws1], "hid-a", false);
        const winH = mkWin(out2, [ws2], "hid-h", true);
        const surface = {
            screens: [out1, out2],
            desktops: [ws1, ws2],
            activeWindow: winA,
            activeScreen: out1,
            currentDesktopForScreen: (o: unknown): object => (o === out2 ? ws2 : ws1),
            clientArea: (): object => ({ x: 0, y: 0, w: 1200, h: 800 }),
            windowList: (): object[] => [winA, winH],
        };
        const homing = new Map<string, { output: string; workspace: string }>();
        const gaps: DomainGaps = { innerGap: 8, outerGap: 8 };
        const seen = observeHiddenDomains(surface, new Map(), new Set(), gaps, undefined, new Map(), homing);
        const hidden = seen.find((e) => e.domainOutput === "out-2" && e.domainWorkspace === "ws-2");
        assert.ok(hidden !== undefined, "hidden domain observed");
        assert.deepEqual(
            hidden.windows.map((w) => w.id),
            ["hid-h"],
            "minimized hidden member stays observed",
        );
        assert.deepEqual(homing.get("hid-h"), { output: "out-2", workspace: "ws-2" }, "hidden homing recorded");
        assert.equal(homing.get("hid-a"), undefined, "foreground member gains no hidden homing");
    });
});
