// Native effect presentation-facts waiter (Stage 3 ingress): the held
// WaitPresentationFacts call delivers versioned snapshots before their
// observation, wakes observation on change without polling, and fences
// stale generations. The effect service is injected; production wires the
// global callDBus.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";
import { EFFECT_FACTS_LEASE_MS } from "../src/world-entry";

interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
    service: string;
    path: string;
    iface: string;
}

function factsReply(snapshot: Record<string, unknown>): string {
    return JSON.stringify({ v: 1, changed: 0, superseded: 0, available: 1, ...snapshot });
}

const SNAP_A = {
    instance: "effect-1",
    rev: "3",
    invalidation: "7",
    chord: 0,
    chordKnown: 1,
    gap: 0.0,
    width: 3.0,
    extension: -1.0,
};

function makeHarness() {
    const ws1 = { id: "ws-1", name: "Workspace 1" };
    const wins: Array<Record<string, unknown>> = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
    ];
    const world = {
        screens: [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out-1" },
        activeWindow: wins[0],
        currentByOut: new Map([["out-1", ws1]]),
    };
    const surface = {
        windowList: () => wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByOut.get(screen.name) ?? null,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => undefined,
        removeDesktop: () => undefined,
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const effectCalls: CapturedCall[] = [];
    let describeRev = 0;
    let gen = "gen-1";
    let describeBehavior: ((call: CapturedCall) => void) | null = null;
    let effectBehavior: (call: CapturedCall) => void = () => undefined;
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback, service: _s, path: _p, iface: _i });
            if (method === "DiscoverWorld") {
                const body = JSON.parse(payload) as { req: string };
                callback(JSON.stringify({
                    v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
                    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                    windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
                    operationSettled: [], nativeCreations: [], warpCursor: null,
                    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                }));
            } else if (method === "DescribeWorld") {
                const call = calls[calls.length - 1] as CapturedCall;
                if (describeBehavior !== null) {
                    describeBehavior(call);
                    return;
                }
                // Production-shaped service: every capture is answered, so
                // no flight ever sticks and dispatches keep flowing. Tests
                // assert on the sent payloads (facts ingress), never on
                // reply control.
                const body = JSON.parse(payload) as { source: string; nativeInvalid: string };
                describeRev += 1;
                callback(JSON.stringify({
                    v: 2, adapter: "broker-1", gen, req: body.source, obs: body.nativeInvalid, rev: String(describeRev),
                    ready: true, consumed: false,
                    source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                    gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
                    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                    scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
                    workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
                }));
            }
        },
        effectCallDbus: (service, path, iface, method, payload, callback) => {
            const call = { method, payload, callback, service, path, iface };
            effectCalls.push(call);
            effectBehavior(call);
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
        log: (m) => {
            logs.push(m);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    // The read-only presentation subscriber arms its own 25 s
    // host-deadline watchdog per read flight (Stage 5, finding A) in the
    // same shared pool. Facts-lane assertions skip that retry timer
    // explicitly: the marker changes neither its delay nor its cadence,
    // and the skipped lane keeps its own coverage in
    // world-presentation-subscriber.test.ts.
    const isPresentationTimer = (t: { delay: number; cb: () => void }): boolean =>
        (t.cb as unknown as Record<string, unknown>)["presentationReadWatchdog"] === true;
    const runTimers = (maxDelay = 120, cap = Number.POSITIVE_INFINITY): void => {
        // Capped sweep (B1 host-timer discipline): firing a 25 s watchdog
        // re-arms another 25 s timer, so an uncapped pass would cascade
        // forever. Short passes run free; host passes fire exactly one.
        let fired = 0;
        for (let i = 0; i < timers.length && fired < cap; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay && !isPresentationTimer(t)) {
                timers.splice(i, 1);
                i -= 1;
                fired += 1;
                t.cb();
            }
        }
    };
    const lastReport = (): Record<string, unknown> => {
        const call = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(call !== undefined);
        return (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
    };
    const describeCount = (): number => calls.filter((c) => c.method === "DescribeWorld").length;
    // The discovery-triggered auto dispatch is answered inline by the fake
    // service; settle here so every test starts from an idle flight.
    runTimers();
    // Fire the most recently scheduled host-deadline timer only (B1
    // discipline): the effect recovery chain is single, so the newest 25 s
    // timer is the live recovery; older ones are inert. This isolates the
    // effect lane from the service host-deadline chain.
    const fireLastHostTimer = (): boolean => {
        for (let i = timers.length - 1; i >= 0; i -= 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay === 25000 && !isPresentationTimer(t)) {
                timers.splice(i, 1);
                t.cb();
                return true;
            }
        }
        return false;
    };
    const answeredEffect = new Set<CapturedCall>();
    const answerEffect = (index: number, reply: unknown): void => {
        const call = effectCalls[index] as CapturedCall | undefined;
        assert.ok(call !== undefined, `expected effect call ${index}`);
        assert.ok(!answeredEffect.has(call), `effect call ${index} already answered`);
        answeredEffect.add(call);
        call.callback(reply);
    };
    return {
        handle, calls, timers, logs, effectCalls, runTimers, lastReport, describeCount,
        fireLastHostTimer, answerEffect,
        setEffectBehavior: (fn: (call: CapturedCall) => void): void => {
            effectBehavior = fn;
        },
        setEffectThrowNext: (): void => {
            const prev = effectBehavior;
            let thrown = false;
            effectBehavior = (call) => {
                if (!thrown) {
                    thrown = true;
                    throw new Error("effect-transport-down");
                }
                prev(call);
            };
        },
        effectRequest: (index: number): Record<string, unknown> => {
            const call = effectCalls[index] as CapturedCall | undefined;
            assert.ok(call !== undefined, `expected effect call ${index}`);
            assert.equal(call.service, "com.omnitiler.ActiveBorder");
            assert.equal(call.method, "WaitPresentationFacts");
            return JSON.parse(call.payload) as Record<string, unknown>;
        },
        setGen: (next: string): void => {
            gen = next;
        },
        setDescribeBehavior: (fn: ((call: CapturedCall) => void) | null): void => {
            describeBehavior = fn;
        },
    };
}

describe("native effect presentation-facts waiter", () => {
    it("arms one held waiter per generation with an empty identity", () => {
        const h = makeHarness();
        h.runTimers();
        assert.equal(h.effectCalls.length, 1);
        assert.deepEqual(h.effectRequest(0), { instance: "", rev: "", leaseMs: EFFECT_FACTS_LEASE_MS });
        h.handle.stop();
    });

    it("publishes the fetched snapshot on the next observation after a changed reply", () => {
        const h = makeHarness();
        h.runTimers();
        h.effectCalls[0]?.callback(factsReply({ ...SNAP_A, changed: 1 }));
        // The changed reply invalidates and dispatches; the fake service
        // answers inline, so the fresh facts precede their observation.
        h.runTimers();
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], true);
        assert.equal(presentation["underlayEnabled"], true);
        assert.equal(presentation["borderGap"], 0);
        assert.equal(presentation["borderWidth"], 3);
        // Sentinel -1 resolves to the border width before rounding.
        assert.equal(presentation["underlayExtension"], 3);
        assert.equal(presentation["chordHeld"], false);
        h.handle.stop();
    });

    it("re-arms with the new identity and skips dispatch on lease expiry", () => {
        const h = makeHarness();
        h.runTimers();
        h.effectCalls[0]?.callback(factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2, "changed reply re-arms immediately");
        assert.deepEqual(h.effectRequest(1), { instance: "effect-1", rev: "3", leaseMs: EFFECT_FACTS_LEASE_MS });
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.effectCalls[1]?.callback(factsReply({ ...SNAP_A, changed: 0 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 3, "lease expiry re-arms without dispatch");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore,
            "an unchanged lease never dispatches by itself",
        );
        h.handle.stop();
    });

    it("invalidates and observes fresh facts on a changed reply", () => {
        const h = makeHarness();
        h.runTimers();
        h.effectCalls[0]?.callback(factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        const describesBefore = h.describeCount();
        assert.equal(h.effectCalls.length, 2);
        h.effectCalls[1]?.callback(factsReply({
            ...SNAP_A, rev: "4", chord: 1, changed: 1,
        }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 3, "changed reply re-arms with the new rev");
        assert.deepEqual(h.effectRequest(2), { instance: "effect-1", rev: "4", leaseMs: EFFECT_FACTS_LEASE_MS });
        assert.ok(
            h.describeCount() > describesBefore,
            "a changed snapshot dispatches a fresh observation",
        );
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["chordHeld"], true);
        h.handle.stop();
    });

    it("drops a late callback after the watchdog re-arms without a sound", () => {
        const h = makeHarness();
        h.runTimers();
        const first = h.effectCalls[0];
        assert.ok(first !== undefined);
        // Host kills the held call: the 25 s watchdog fires unknown and
        // re-arms with the same identity (one fire only; the re-armed
        // waiter parks without further timers). The watchdog's own
        // invalidate+dispatch carries the hidden presentation.
        h.runTimers(25000, 1);
        assert.equal(h.effectCalls.length, 2, "watchdog re-arms the waiter");
        h.runTimers();
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], false, "unknown facts hide instead of guessing");
        // The late original reply fences on its settled transport slot and
        // never dispatches, silently: a lost acknowledgement is not work.
        const describesBefore = h.describeCount();
        first.callback(factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.describeCount(), describesBefore);
        assert.ok(!h.logs.some((l) => l.includes("effect-facts-stale")));
        h.handle.stop();
    });

    it("drops a replaced waiter callback across a generation rotation", () => {
        const h = makeHarness();
        h.runTimers();
        h.effectCalls[0]?.callback(factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        // Force a generation mismatch on the next capture: the entry
        // rediscovers under a new generation, dropping the old waiter slot
        // (spend/no replay across the rotation; the snapshot resets).
        h.setGen("gen-2");
        let mismatched = false;
        h.setDescribeBehavior((call) => {
            const body = JSON.parse(call.payload) as { source: string; nativeInvalid: string };
            if (!mismatched) {
                mismatched = true;
                call.callback(JSON.stringify({
                    v: 2, outcome: "rejected", kind: "generation-mismatch", message: "superseded",
                    gen: "gen-1", source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                }));
                return;
            }
            h.setDescribeBehavior(null);
            // Fall through to a production-shaped accept for this call.
            const rev = String(100 + h.describeCount());
            call.callback(JSON.stringify({
                v: 2, adapter: "broker-1", gen: "gen-2", req: body.source, obs: body.nativeInvalid, rev,
                ready: true, consumed: false,
                source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
                workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
            }));
        });
        // Trigger a dispatch carrying the new facts; the mismatch reply
        // rotates the generation and parks a fresh waiter.

        h.effectCalls[1]?.callback(factsReply({ ...SNAP_A, rev: "4", chord: 1, changed: 1 }));
        h.runTimers();
        h.runTimers();
        // The changed reply re-arms first (waiter 3, gen-1 identity); the
        // mismatch then rotates the generation and parks a fresh waiter
        // (waiter 4, empty identity). Both superseded gen-1 waiters fence.
        assert.equal(h.effectCalls.length, 4, "rotation parks a fresh waiter");
        assert.deepEqual(h.effectRequest(3), { instance: "", rev: "", leaseMs: EFFECT_FACTS_LEASE_MS });
        // The superseded gen-1 waiter (armed by the changed reply, never
        // answered before the rotation) reply fences on the generation:
        // no snapshot resurrection, no dispatch.
        const describesBefore = h.describeCount();
        const superseded = h.effectCalls[2];
        assert.ok(superseded !== undefined);
        superseded.callback(factsReply({ ...SNAP_A, rev: "9", chord: 1, changed: 1 }));
        h.runTimers();
        assert.equal(h.describeCount(), describesBefore);
        assert.ok(h.logs.some((l) => l.includes("effect-facts-stale")));
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], false, "rotated generation hides until fresh facts");
        h.handle.stop();
    });

    it("keeps the newer watchdog when a rotated waiter answers late", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2);
        // Same rotation dance as above: the changed reply re-arms a gen-1
        // waiter, then the generation-mismatch rotates and parks a fresh
        // gen-2 waiter with its own watchdog.
        h.setGen("gen-2");
        let mismatched = false;
        h.setDescribeBehavior((call) => {
            const body = JSON.parse(call.payload) as { source: string; nativeInvalid: string };
            if (!mismatched) {
                mismatched = true;
                call.callback(JSON.stringify({
                    v: 2, outcome: "rejected", kind: "generation-mismatch", message: "superseded",
                    gen: "gen-1", source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                }));
                return;
            }
            h.setDescribeBehavior(null);
            const rev = String(100 + h.describeCount());
            call.callback(JSON.stringify({
                v: 2, adapter: "broker-1", gen: "gen-2", req: body.source, obs: body.nativeInvalid, rev,
                ready: true, consumed: false,
                source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
                workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
            }));
        });
        h.answerEffect(1, factsReply({ ...SNAP_A, rev: "4", chord: 1, changed: 1 }));
        h.runTimers();
        h.runTimers();
        assert.equal(h.effectCalls.length, 4, "rotation parks a fresh waiter");
        // The late gen-1 waiter fences in the wrapper: no dispatch, and the
        // gen-2 arm keeps its watchdog instead of losing its timeout.
        const describesBefore = h.describeCount();
        h.answerEffect(2, factsReply({ ...SNAP_A, rev: "9", chord: 1, changed: 1 }));
        h.runTimers();
        assert.equal(h.describeCount(), describesBefore, "stale callback never dispatches");
        assert.ok(h.logs.some((l) => l.includes("effect-facts-stale")));
        assert.ok(h.fireLastHostTimer(), "newer watchdog survived the stale callback");
        assert.equal(h.effectCalls.length, 5, "surviving watchdog re-arms once");
        h.handle.stop();
    });

    it("hides on a malformed reply and recovers only on the host deadline", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        const describesBefore = h.describeCount();
        const presentationBefore = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentationBefore["borderWidth"], 3);
        assert.equal(h.effectCalls.length, 2);
        // One malformed payload: the snapshot clears to unknown with one
        // follow-up capture (hide), then the lane stands down instead of
        // re-arming immediately.
        h.answerEffect(1, JSON.stringify({ v: 1, bogus: true }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2, "malformed reply never re-arms immediately");
        assert.equal(
            h.describeCount(),
            describesBefore + 1,
            "exactly one follow-up capture publishes the hide",
        );
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], false, "unknown facts hide instead of guessing");
        assert.equal(presentation["borderWidth"], 0);
        assert.ok(h.logs.some((l) => l.includes("effect-facts-malformed")));
        // Valid recovery resets through the host deadline, not a hot loop.
        assert.ok(h.fireLastHostTimer(), "recovery chain armed");
        assert.equal(h.effectCalls.length, 3, "recovery re-arms once per host deadline");
        h.answerEffect(2, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        const recovered = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(recovered["borderWidth"], 3, "valid recovery restores facts");
        h.handle.stop();
    });

    it("bounds a persistently malformed source to host-deadline cadence, never a storm", () => {
        const h = makeHarness();
        h.runTimers();
        // Every armed waiter gets a malformed answer, answered explicitly
        // (no auto-answer hot loop possible: one reply per arm).
        const malformed = JSON.stringify({ v: 1, bogus: true });
        for (let round = 0; round < 3; round += 1) {
            const callsBefore = h.effectCalls.length;
            const describesBefore = h.describeCount();
            h.answerEffect(h.effectCalls.length - 1, malformed);
            h.runTimers();
            // Answering consumes the waiter with no re-arm; the single host
            // deadline fires exactly one recovery re-arm.
            assert.ok(h.fireLastHostTimer(), `round ${round}: recovery chain armed`);
            h.runTimers();
            const newCalls = h.effectCalls.length - callsBefore;
            const newDescribes = h.describeCount() - describesBefore;
            assert.ok(newCalls >= 1 && newCalls <= 2, `round ${round}: bounded calls, got ${newCalls}`);
            assert.ok(newDescribes >= 1 && newDescribes <= 2, `round ${round}: bounded captures, got ${newDescribes}`);
            const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
            assert.equal(presentation["borderEnabled"], false, `round ${round}: stays hidden`);
        }
        // Between deadlines nothing re-arms: short-timer passes are quiet.
        const idleCalls = h.effectCalls.length;
        h.runTimers();
        h.runTimers();
        assert.equal(h.effectCalls.length, idleCalls, "no hot re-arm between deadlines");
        h.handle.stop();
    });

    it("treats oversize replies as malformed without state change", () => {
        const h = makeHarness();
        h.runTimers();
        h.effectCalls[0]?.callback(factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2);
        h.effectCalls[1]?.callback(JSON.stringify({ v: 1, instance: "effect-1", rev: "x".repeat(5000) }));
        h.runTimers();
        // Oversize (over the reply bound) refuses like malformed: hidden
        // unknown plus host-deadline recovery, no immediate re-arm.
        assert.equal(h.effectCalls.length, 2, "oversize reply never re-arms immediately");
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], false);
        h.handle.stop();
    });

    it("stands down to the host deadline on superseded without re-arming", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2);
        const describesBefore = h.describeCount();
        // A superseded waiter (replaced by a newer hold) stands down: no
        // snapshot change, no invalidation, and exactly one host-deadline
        // retry instead of an immediate ping-pong re-arm.
        h.answerEffect(1, factsReply({ ...SNAP_A, changed: 0, superseded: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2, "superseded never re-arms immediately");
        assert.equal(h.describeCount(), describesBefore, "superseded never dispatches");
        assert.ok(h.logs.some((l) => l.includes("effect-facts-superseded")));
        const kept = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(kept["borderWidth"], 3, "supersession keeps the snapshot");
        assert.ok(h.fireLastHostTimer(), "stand-down arms one host-deadline retry");
        assert.equal(h.effectCalls.length, 3, "stand-down re-arms on the host deadline");
        h.handle.stop();
    });

    it("hides presentation when the effect never answers", () => {
        const h = makeHarness();
        h.setEffectBehavior(() => undefined);
        h.runTimers();
        const presentation = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(presentation["borderEnabled"], false);
        assert.equal(presentation["chordHeld"], false);
        h.handle.stop();
    });

    it("drops the snapshot once on a synchronous transport throw, then stands down", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        const shown = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(shown["borderWidth"], 3);
        assert.equal(h.effectCalls.length, 2);
        // The next native send throws synchronously: the retained snapshot
        // is dropped with one follow-up capture (hide), then the lane
        // stands down to the host deadline instead of re-arming. The failed
        // send is recorded but holds no waiter.
        h.setEffectThrowNext();
        h.answerEffect(1, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 3, "throw records the attempt but holds no waiter");
        const hidden = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(hidden["borderEnabled"], false, "throw drops the snapshot to unknown");
        assert.ok(h.logs.some((l) => l.includes("effect-facts-transport")));
        assert.ok(h.fireLastHostTimer(), "throw stands down to the host deadline");
        assert.equal(h.effectCalls.length, 4, "host deadline re-arms once");
        h.answerEffect(3, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        const recovered = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(recovered["borderWidth"], 3, "valid answer after the throw restores facts");
        h.handle.stop();
    });

    it("refuses a chord claim without known and folds unknown chord to false", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2);
        // chord:1 with chordKnown:0 is contradictory: the script cannot
        // prove a level it reports as unknown, so the whole reply refuses
        // like malformed (unknown + host-deadline recovery).
        h.answerEffect(1, factsReply({ ...SNAP_A, chord: 1, chordKnown: 0, changed: 1 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2, "contradictory chord never re-arms immediately");
        const hidden = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(hidden["borderEnabled"], false);
        assert.ok(h.logs.some((l) => l.includes("effect-facts-malformed")));
        // Unknown chord with no claim rides false, never a guess.
        assert.ok(h.fireLastHostTimer());
        h.answerEffect(2, factsReply({ ...SNAP_A, chord: 0, chordKnown: 0, changed: 1 }));
        h.runTimers();
        const shown = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(shown["borderEnabled"], true);
        assert.equal(shown["chordHeld"], false, "unknown chord folds to false");
        h.handle.stop();
    });

    it("hides on an explicit unavailable snapshot and recovers on the host deadline", () => {
        const h = makeHarness();
        h.runTimers();
        h.answerEffect(0, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal((h.lastReport()["presentation"] as Record<string, unknown>)["borderEnabled"], true);
        // The effect reports its own teardown: unlike a superseded waiter
        // (kept snapshot), an unavailable endpoint hides immediately.
        h.answerEffect(1, factsReply({ ...SNAP_A, changed: 0, available: 0 }));
        h.runTimers();
        assert.equal(h.effectCalls.length, 2, "unavailable never re-arms immediately");
        const hidden = h.lastReport()["presentation"] as Record<string, unknown>;
        assert.equal(hidden["borderEnabled"], false, "unavailable hides, never stale");
        assert.ok(h.logs.some((l) => l.includes("effect-facts-unavailable")));
        assert.ok(h.fireLastHostTimer(), "unavailable stands down to the host deadline");
        assert.equal(h.effectCalls.length, 3);
        h.answerEffect(2, factsReply({ ...SNAP_A, changed: 1 }));
        h.runTimers();
        assert.equal((h.lastReport()["presentation"] as Record<string, unknown>)["borderEnabled"], true, "fresh endpoint restores facts");
        h.handle.stop();
    });
});
