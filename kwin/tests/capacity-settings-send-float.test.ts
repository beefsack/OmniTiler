// D2/D3 capacity settings plus the D3 capacity-float send route offline
// coverage: tolerant TS readers with defaults, live per-dispatch re-read
// timing, report JSON carriage, and the `move-to-float` workspace-send
// validation/actuation (survivor geometry only, mover frame untouched,
// follow once on arrival proof; forged held-mover and leafless shapes
// refuse). Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    ANCHOR_FALLBACK_DEFAULT,
    createWorldSettingsLive,
    normalizeAnchorFallback,
    normalizeNoSpaceResult,
    NO_SPACE_RESULT_DEFAULT,
} from "../src/world-settings-live";
import { buildWorldReportJson } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";
import {
    WORKSPACE_SEND_CONTRACT_VERSION,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WorkspaceSendAdapter,
    type WorkspaceSendAdapterEnv,
    type WorkspaceSendObserved,
} from "../src/workspace-send-adapter";

describe("capacity settings tolerant readers", () => {
    it("default to largest-splittable-window and float", () => {
        assert.equal(ANCHOR_FALLBACK_DEFAULT, "largest-splittable-window");
        assert.equal(NO_SPACE_RESULT_DEFAULT, "float");
        assert.equal(normalizeAnchorFallback(undefined), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeNoSpaceResult(undefined), NO_SPACE_RESULT_DEFAULT);
    });

    it("accepts exactly the catalog tokens while rejecting the rest", () => {
        assert.equal(normalizeAnchorFallback("largest-splittable-window"), "largest-splittable-window");
        assert.equal(normalizeAnchorFallback("closest-splittable-relative"), "closest-splittable-relative");
        assert.equal(normalizeAnchorFallback("no-search"), "no-search");
        assert.equal(normalizeAnchorFallback(""), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback("largest"), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback("no_search"), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback(0), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback(null), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeNoSpaceResult("float"), "float");
        assert.equal(normalizeNoSpaceResult("allow-overlap"), "allow-overlap");
        assert.equal(normalizeNoSpaceResult(""), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult("overlap"), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult("allow_overlap"), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult(false), NO_SPACE_RESULT_DEFAULT);
    });
});

describe("capacity settings live timing", () => {
    it("re-resolves per dispatch with no restart, tolerating failures", () => {
        let anchor: unknown = "largest-splittable-window";
        let noSpace: unknown = "float";
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => anchor,
            readNoSpaceResultFn: () => noSpace,
        });
        assert.equal(settings.readAnchorFallbackLive(), "largest-splittable-window");
        assert.equal(settings.readNoSpaceResultLive(), "float");
        // A deliberate Options reload applies to the next dispatch.
        anchor = "closest-splittable-relative";
        noSpace = "allow-overlap";
        assert.equal(settings.readAnchorFallbackLive(), "closest-splittable-relative");
        assert.equal(settings.readNoSpaceResultLive(), "allow-overlap");
        // Invalid values backfill instead of sticking.
        anchor = "bogus";
        noSpace = "bogus";
        assert.equal(settings.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(settings.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
    });

    it("falls back when readers throw or config is unreadable", () => {
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => {
                throw new Error("boom");
            },
            readNoSpaceResultFn: () => {
                throw new Error("boom");
            },
        });
        assert.equal(settings.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(settings.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
        // No overrides and no KWin readConfig in node: unreadable config
        // backfills the defaults instead of throwing.
        const bare = createWorldSettingsLive();
        assert.equal(bare.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(bare.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
    });
});

function minimalObservation(): WorldObservation {
    return {
        mode: "per-output-local",
        topology: [],
        bindings: [],
        domains: [],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
    };
}

describe("capacity settings report carriage", () => {
    it("encodes validated tokens on every full report", () => {
        const json = buildWorldReportJson({
            observation: minimalObservation(),
            reqSeq: "1",
            cmdSeq: null,
            reqId: "req-1",
            command: null,
            owner: "owner-a",
            adapter: "adapter-1",
            boundGen: "gen-1",
            currentObs: "1",
            pendingFeedback: [],
            defaultTiled: true,
            refill: "last-remaining-workspace",
            anchorFallback: "closest-splittable-relative",
            noSpaceResult: "allow-overlap",
        });
        assert.ok(json !== null);
        const body = JSON.parse(json as string) as Record<string, unknown>;
        assert.equal(body["anchorFallback"], "closest-splittable-relative");
        assert.equal(body["noSpaceResult"], "allow-overlap");
    });

    it("round-trips the documented wire alphabet shape", () => {
        const json = buildWorldReportJson({
            observation: minimalObservation(),
            reqSeq: "1",
            cmdSeq: null,
            reqId: "req-1",
            command: null,
            owner: "owner-a",
            adapter: "adapter-1",
            boundGen: "gen-1",
            currentObs: "1",
            pendingFeedback: [],
            defaultTiled: true,
            refill: "last-remaining-workspace",
            anchorFallback: "largest-splittable-window",
            noSpaceResult: "float",
        });
        assert.ok(json !== null);
        const text = json as string;
        assert.ok(text.includes("\"anchorFallback\":\"largest-splittable-window\""));
        assert.ok(text.includes("\"noSpaceResult\":\"float\""));
    });
});

// ---------- move-to-float send harness (trimmed production shape) ----------

function makeRefs(): { a: object; b: object; t: object; desktop: object } {
    return { a: {}, b: {}, t: {}, desktop: {} };
}

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

interface HarnessWindow {
    readonly id: string;
    readonly ref: object;
    rect: { x: number; y: number; w: number; h: number };
    workspace: string;
}

interface Harness {
    readonly windows: HarnessWindow[];
    activeRef: object | null;
}

function harnessObserved(harness: Harness, refs: { a: object; b: object; t: object; desktop: object }): WorkspaceSendObserved {
    const sourceWindows = Object.freeze(
        harness.windows
            .filter((entry) => entry.workspace === "ws-1")
            .map((entry) =>
                Object.freeze({ id: entry.id, ref: entry.ref, rect: Object.freeze({ ...entry.rect }) }),
            ),
    );
    const targetWindows = Object.freeze(
        harness.windows
            .filter((entry) => entry.workspace === "ws-2")
            .map((entry) =>
                Object.freeze({ id: entry.id, ref: entry.ref, rect: Object.freeze({ ...entry.rect }) }),
            ),
    );
    const activeId = harness.windows.find((entry) => entry.ref === harness.activeRef)?.id ?? "";
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
        activeRef: harness.activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: "ws-1",
    };
}

interface Mocks {
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly geometries: Array<{ target: object; rect: { x: number; y: number; w: number; h: number } }>;
    readonly desktops: Array<{ target: object; refs: ReadonlyArray<object> }>;
    readonly switches: object[];
    readonly focuses: object[];
    readonly logs: string[];
    readonly harness: Harness;
    observeImpl: () => WorkspaceSendObserved | null;
    holdArrival: boolean;
    settled: number;
    env: WorkspaceSendAdapterEnv;
}

function mockEnv(refs: { a: object; b: object; t: object; desktop: object }): Mocks {
    const state = {
        callbacks: [],
        geometries: [],
        desktops: [],
        switches: [],
        focuses: [],
        logs: [],
        harness: {
            windows: [
                { id: "win-a", ref: refs.a, rect: rect(0, 0, 100, 100), workspace: "ws-1" },
                { id: "win-b", ref: refs.b, rect: rect(100, 0, 100, 100), workspace: "ws-1" },
                { id: "win-t", ref: refs.t, rect: rect(0, 0, 100, 100), workspace: "ws-2" },
            ],
            activeRef: refs.a,
        } as Harness,
        observeImpl: null as unknown as Mocks["observeImpl"],
        holdArrival: false,
        settled: 0,
        env: null as unknown as WorkspaceSendAdapterEnv,
    } as Mocks;
    state.observeImpl = () => harnessObserved(state.harness, refs);
    const env: WorkspaceSendAdapterEnv = {
        callDbus: (_service, _path, _iface, method, _payload, callback) => {
            if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            state.callbacks.push(callback);
        },
        scheduleOnce: () => () => undefined,
        log: (message) => {
            state.logs.push(message);
        },
        observe: () => state.observeImpl(),
        onSettled: () => {
            state.settled += 1;
        },
        setGeometry: (target, r) => {
            state.geometries.push({ target, rect: r });
            for (const entry of state.harness.windows) {
                if (entry.ref === target) {
                    entry.rect = { x: r.x, y: r.y, w: r.w, h: r.h };
                }
            }
            return true;
        },
        setDesktops: (target, refsArg) => {
            state.desktops.push({ target, refs: refsArg });
            if (!state.holdArrival) {
                for (const entry of state.harness.windows) {
                    if (entry.ref === target) {
                        entry.workspace = "ws-2";
                    }
                }
            }
            return true;
        },
        switchToTarget: (desktopRef) => {
            state.switches.push(desktopRef);
            return true;
        },
        focusWindow: (windowRef) => {
            state.focuses.push(windowRef);
            return true;
        },
        subscribeMoverDesktops: () => () => undefined,
    };
    state.env = env;
    return state;
}

const KNOWN_PRECONDITIONS = [
    "window-observed",
    "desired-topology-valid",
    "adapter-must-verify-postconditions",
];

// D3 capacity-float reply: the tiled mover drops its source leaf and floats
// at the destination; survivors (mover excluded) carry the geometry.
function floatReply(correlation: string, operation: Record<string, unknown>): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-b", leaf: "leaf-win-b", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-1", workspace: "ws-2", rect: { x: 0, y: 0, w: 100, h: 100 } },
        ],
        desired_focus: { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-t" },
        preconditions: KNOWN_PRECONDITIONS,
        operation,
    });
}

function moveToFloatOp(follow: boolean): Record<string, unknown> {
    return {
        op: "move-to-float",
        window: "win-a",
        leaf: "leaf-win-a",
        source_output: "out-1",
        source_workspace: "ws-1",
        target_output: "out-1",
        target_workspace: "ws-2",
        follow,
    };
}

function dispatch(mocks: Mocks, adapter: WorkspaceSendAdapter, follow = true): string {
    assert.equal(adapter.requestSend("ws-2", undefined, follow), true);
    mocks.callbacks[0]?.(":1.7");
    // The request correlation is internal; recover it from the dispatch log.
    const line = mocks.logs.find((l) => l.includes("outcome=started"));
    assert.ok(line !== undefined, mocks.logs.join("\n"));
    const match = /correlation=([A-Za-z0-9_.-]+)/.exec(line as string);
    assert.ok(match !== null);
    return match[1] as string;
}

describe("move-to-float send route", () => {
    it("actuates survivor geometry plus mover membership with no mover frame write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter);
        const moverFrame = { ...mocks.harness.windows[0]?.rect } as { x: number; y: number; w: number; h: number };
        mocks.callbacks[1]?.(floatReply(correlation, moveToFloatOp(true)));
        // Collapsed-source survivor only: win-b reflows while the untouched
        // target (win-t) takes no write. The mover takes no frame write
        // and keeps its native same-output frame verbatim.
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.geometries[0]?.target, refs.b);
        assert.ok(!mocks.geometries.some((g) => g.target === refs.a));
        assert.ok(!mocks.geometries.some((g) => g.target === refs.t));
        assert.deepEqual(mocks.harness.windows[0]?.rect, moverFrame);
        assert.equal(mocks.desktops.length, 1);
        assert.equal(mocks.desktops[0]?.target, refs.a);
        // Immediate arrival proof: switch then focus, exactly once.
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [refs.a]);
        assert.equal(mocks.settled, 1);
        assert.equal(adapter.isInFlight, false);
    });

    it("refuses a held mover claiming a dropped leaf (forgery protection)", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // The snapshot mover is an ordinary intentional float: a
        // move-to-float naming it is mismatched (that shape is
        // move-deferred's), never actuated.
        mocks.observeImpl = () => {
            const live = harnessObserved(mocks.harness, refs);
            return {
                ...live,
                sourceWindows: Object.freeze(
                    live.sourceWindows.map((entry) =>
                        entry.id === "win-a"
                            ? Object.freeze({
                                  ...entry,
                                  floating: true,
                                  fitExcluded: true,
                                  fit_excluded: true,
                              })
                            : entry,
                    ),
                ),
            };
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(floatReply(correlation, moveToFloatOp(true)));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=precondition-mismatch")), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
    });

    it("refuses a leafless move-to-float as malformed", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter);
        const leafless = { ...moveToFloatOp(true) };
        delete leafless["leaf"];
        mocks.callbacks[1]?.(floatReply(correlation, leafless));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=precondition-mismatch")), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
    });

    it("stays bind the source MRU without touching the dropped leaf", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter, false);
        const stay = JSON.parse(floatReply(correlation, moveToFloatOp(false))) as Record<string, unknown>;
        // Stay names the source survivor MRU, never the dropped leaf.
        stay["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-b" };
        mocks.callbacks[1]?.(JSON.stringify(stay));
        // Collapsed-source survivor write only; the mover keeps its frame.
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.geometries[0]?.target, refs.b);
        assert.equal(mocks.desktops.length, 1);
        // Stay keeps the source view: no target switch; the source MRU
        // survivor takes focus, never the floated mover or the dropped leaf.
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, [refs.b]);
        assert.equal(mocks.settled, 1);
    });
});
