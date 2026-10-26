// Stage 5 production-route fixture proof (JS legs, GREEN).
//
// Facts -> capture (production entry route): the verbatim
// `test-fixtures/stage5/facts-seed.json` waiter bytes are answered through
// the real `startWorldAdapterEntry` facts lane (real parse, real
// captureWorldObservation, real envelope builders); the emitted
// DescribeWorld witness lane must equal the witness lane of the recorded
// `envelope-p5.json` bytes (same lane, built by the same real code) and
// the covered witness of the recorded `projection-p5.json` bytes.
//
// Projection -> dispatch (production subscriber route): the recorded
// `projection-*.json` broker bytes are routed through the real
// `createWorldPresentationSubscriber`; each rendered payload must equal
// the recorded `dispatch-*.json` bytes exactly. Fresh legs also pin the
// already-correct fencing: cross-gesture preview strip and stale-coverage
// skip (both GREEN against the unchanged runtime).
//
// Correlation seam (disclosed, shape-preserving): the subscriber mints a
// per-read `req` nonce, so the test answers each read with the recorded
// projection bytes re-keyed to the issued `req` only. The native dispatch
// payload carries no `req`, so rendered bytes stay exactly comparable.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { startWorldAdapterEntry } from "../src/world-entry";
import { createWorldPresentationSubscriber } from "../src/world-presentation";

const GEN = "gen-stage5";
const ADAPTER = "kwin-actuator-1";

function fixture(name: string): string {
    return readFileSync(resolve("..", "test-fixtures", "stage5", name), "utf8");
}

// --- Part A: production entry harness (facts waiter -> DescribeWorld). ---

interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

function makeEntryHarness(): {
    calls: CapturedCall[];
    effectCalls: CapturedCall[];
    runTimers: () => void;
    lastReport: () => Record<string, unknown>;
    stop: () => void;
} {
    const desk = { id: "550e8400-e29b-41d4-a716-446655440001", name: "Workspace 1" };
    const out = { name: "DP-1" };
    const mkWin = (
        id: string,
        frame: { x: number; y: number; width: number; height: number },
    ): Record<string, unknown> => ({
        internalId: id,
        normalWindow: true,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        frameGeometry: { ...frame },
        clientGeometry: { ...frame },
        output: out,
        desktops: [desk],
        minimized: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        minSize: null,
        maxSize: null,
    });
    const wins = [
        mkWin("win-a", { x: 0, y: 0, width: 200, height: 200 }),
        mkWin("win-b", { x: 300, y: 0, width: 200, height: 200 }),
    ];
    const winA = wins[0] as Record<string, unknown>;
    const calls: CapturedCall[] = [];
    const effectCalls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    let rev = 0;
    const handle = startWorldAdapterEntry({
        workspace: {
            windowList: () => wins as never,
            screens: [{ name: "DP-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }] as never,
            desktops: [desk] as never,
            activeScreen: out as never,
            get activeWindow() {
                return winA as never;
            },
            cursorPos: { x: 150, y: 50 },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktop: desk as never,
            currentDesktopForScreen: () => desk as never,
            setCurrentDesktopForScreen: () => undefined,
            createDesktop: () => undefined,
            removeDesktop: () => undefined,
        } as never,
        owner: "owner-a",
        adapter: ADAPTER,
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "DiscoverWorld") {
                const body = JSON.parse(payload) as { req: string };
                callback(JSON.stringify({
                    v: 2, adapter: "broker-1", gen: GEN, req: body.req, obs: "0", rev: "0",
                    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                    windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
                    operationSettled: [], nativeCreations: [], warpCursor: null,
                    presentation: {
                        foreground: "",
                        border: { visible: false, reason: "disabled", inner: null, outer: null },
                        underlay: { status: "hidden", reason: "disabled", baseRev: null },
                        pager: [],
                    },
                }));
            } else if (method === "DescribeWorld") {
                const body = JSON.parse(payload) as { source: string; nativeInvalid: string };
                rev += 1;
                callback(JSON.stringify({
                    v: 2, adapter: "broker-1", gen: GEN, req: body.source, obs: body.nativeInvalid,
                    rev: String(rev), ready: true, consumed: false,
                    source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                    gestureSettled: null, gestureAssoc: null, operationSettled: [],
                    nativeCreations: [], warpCursor: null,
                    presentation: {
                        foreground: "",
                        border: { visible: false, reason: "disabled", inner: null, outer: null },
                        underlay: { status: "hidden", reason: "disabled", baseRev: null },
                        pager: [],
                    },
                    scopes: [], workspaces: [], outputs: [], windows: [], transfers: [],
                    effects: [], result: null,
                }));
            }
        },
        effectCallDbus: (_s, _p, _i, method, payload, callback) => {
            effectCalls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const timer = { delay, cb };
            timers.push(timer);
            return () => {
                const index = timers.indexOf(timer);
                if (index >= 0) {
                    timers.splice(index, 1);
                }
            };
        },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    const isPresentationTimer = (t: { delay: number; cb: () => void }): boolean =>
        (t.cb as unknown as Record<string, unknown>)["presentationReadWatchdog"] === true;
    return {
        calls,
        effectCalls,
        runTimers: () => {
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120 && !isPresentationTimer(t)) {
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        },
        lastReport: () => {
            const call = [...calls].reverse().find((c) => c.method === "DescribeWorld");
            assert.ok(call !== undefined, "a capture must have been dispatched");
            return (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
        },
        stop: () => {
            handle.stop();
        },
    };
}

// --- Part B: production subscriber harness (ReadWorldPresentation). ---

function makeSubscriberHarness(opts: {
    invalidation: { instance: string; counter: string } | null;
    gesture: { instance: string; serial: string; coreId: string } | null;
}): {
    paints: string[];
    answerFixture: (name: string) => void;
    stop: () => void;
} {
    const pending: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
    const paints: string[] = [];
    const sub = createWorldPresentationSubscriber({
        callDbus: (_s, _p, _i, _m, payload, callback) => {
            pending.push({ payload, callback });
        },
        getGeneration: () => GEN,
        getInvalidation: () => opts.invalidation,
        getGestureIdentity: () => opts.gesture,
        isStopped: () => false,
        log: () => undefined,
        scheduleOnce: () => () => undefined,
        renderer: {
            renderPresentation: (payload) => {
                paints.push(payload);
                return true;
            },
            hidePresentation: () => undefined,
            clearPresentation: () => undefined,
        },
    });
    sub.notifyGeneration(GEN);
    return {
        paints,
        answerFixture: (name: string) => {
            for (const call of pending.splice(0)) {
                const issued = (JSON.parse(call.payload) as { req: string }).req;
                const recorded = JSON.parse(fixture(name)) as Record<string, unknown>;
                recorded["req"] = issued;
                call.callback(JSON.stringify(recorded));
            }
        },
        stop: () => {
            sub.stop();
        },
    };
}

describe("stage5 recorded fixtures (production JS legs)", () => {
    it("facts bytes parse and build the recorded capture witness", () => {
        const h = makeEntryHarness();
        try {
            h.runTimers();
            // Answer the held facts waiter with the verbatim recorded bytes.
            const seed = fixture("facts-seed.json");
            const waiter = [...h.effectCalls].reverse().find((c) => c.method === "WaitPresentationFacts");
            assert.ok(waiter !== undefined, "a facts waiter must be held");
            waiter.callback(seed);
            h.runTimers();
            const report = h.lastReport();
            const witness = report["nativeInvalidWitness"] as { instance: string; counter: string };
            assert.deepEqual(witness, { instance: "effect-1", counter: "0" });
            // The recorded envelope carries the same witness lane, built by
            // the same real code over the same real facts bytes.
            const recorded = JSON.parse(fixture("envelope-p5.json")) as {
                report: { nativeInvalidWitness: unknown };
                nativeInvalid: unknown;
            };
            assert.deepEqual(recorded.report.nativeInvalidWitness, witness);
            // The transport watermark stays a free value (deliberately
            // diverged from the causal witness); only its shape is pinned.
            assert.match(String(recorded.nativeInvalid), /^[0-9]+$/);
            // The recorded projection covers exactly that witness.
            const projection = JSON.parse(fixture("projection-p5.json")) as Record<string, unknown>;
            assert.equal(projection["coveredInstance"], witness.instance);
            assert.equal(projection["coveredInvalid"], witness.counter);
        } finally {
            h.stop();
        }
    });

    it("recorded p5 projection renders the recorded dispatch bytes", () => {
        const h = makeSubscriberHarness({
            invalidation: { instance: "effect-1", counter: "0" },
            gesture: null,
        });
        try {
            h.answerFixture("projection-p5.json");
            assert.equal(h.paints.length, 1);
            assert.equal(h.paints[0], fixture("dispatch-p5.json"));
        } finally {
            h.stop();
        }
    });

    it("recorded covering projection renders the recorded dispatch bytes", () => {
        const h = makeSubscriberHarness({
            invalidation: { instance: "effect-1", counter: "1" },
            gesture: null,
        });
        try {
            h.answerFixture("projection-cover.json");
            assert.equal(h.paints.length, 1);
            assert.equal(h.paints[0], fixture("dispatch-cover.json"));
        } finally {
            h.stop();
        }
    });

    it("recorded live-gesture preview renders with its producing identity", () => {
        const h = makeSubscriberHarness({
            invalidation: { instance: "effect-1", counter: "1" },
            gesture: { instance: "gesture-1", serial: "1", coreId: "1" },
        });
        try {
            h.answerFixture("projection-preview.json");
            assert.equal(h.paints.length, 1);
            assert.equal(h.paints[0], fixture("dispatch-preview.json"));
            const rendered = JSON.parse(h.paints[0] as string) as {
                preview: { producerInstance: string; producerSerial: string };
            };
            assert.equal(rendered.preview.producerInstance, "gesture-1");
            assert.equal(rendered.preview.producerSerial, "1");
        } finally {
            h.stop();
        }
    });

    it("cross-gesture preview strips while border/underlay still paint", () => {
        const h = makeSubscriberHarness({
            invalidation: { instance: "effect-1", counter: "1" },
            gesture: { instance: "gesture-1", serial: "2", coreId: "2" },
        });
        try {
            h.answerFixture("projection-preview.json");
            assert.equal(h.paints.length, 1, "border/underlay still paint from the accepted projection");
            const rendered = JSON.parse(h.paints[0] as string) as {
                preview: unknown;
                border: { visible: number };
            };
            assert.equal(rendered.preview, null, "A preview drained inside B must hide");
            assert.equal(rendered.border.visible, 1);
        } finally {
            h.stop();
        }
    });

    it("stale coverage never paints without a covering capture", () => {
        const h = makeSubscriberHarness({
            invalidation: { instance: "effect-1", counter: "2" },
            gesture: null,
        });
        try {
            h.answerFixture("projection-cover.json");
            assert.equal(h.paints.length, 0, "coverage 1 predates witness 2: no paint");
        } finally {
            h.stop();
        }
    });
});
