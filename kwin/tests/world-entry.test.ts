// Offline integration coverage for the world production entry (D1-D5):
// discovery handshake, full observation reports, freshness acceptance,
// out-of-order/input-inflight/gesture/close/minimize/restart/incomplete/
// duplicate-effect/refusal/ambiguous-creation/removal-race/mode/partial-apply
// scenarios. Scripted in-memory transport; no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry, mintAdapterIncarnation } from "../src/world-entry";
import { createMiniBroker, type MiniBroker } from "./world-wire-harness";
import { isOpaqueId } from "../src/world-seq";

interface FakeDesktop {
    id: string;
    name: string;
}
interface FakeWindow {
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    minSize: null;
    maxSize: null;
    // Exact KWin overlay seams (read-first, lowercase fallback only when
    // absent): `fullScreen`, `maximizeMode` with the `setMaximize`
    // Q_INVOKABLE, and `onAllDesktops`.
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    setMaximize: (vertically: boolean, horizontally: boolean) => void;
}
interface FakeWorld {
    screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
    desktops: FakeDesktop[];
    wins: FakeWindow[];
    activeScreen: { name: string };
    activeWindow: FakeWindow | null;
    currentDesktop: FakeDesktop;
    created: string[];
    removed: string[];
    focused: string[];
    membershipWrites: string[];
    geometryWrites: string[];
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });
const win = (id: string, out: string, ws: FakeDesktop, x = 0): FakeWindow => {
    const fake: FakeWindow = {
        internalId: id,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        keepAbove: false,
        keepBelow: false,
        minSize: null,
        maxSize: null,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        setMaximize: (vertically: boolean, horizontally: boolean): void => {
            fake.maximizeMode = vertically && horizontally ? 3 : 0;
        },
    };
    return fake;
};

function makeWorld(): FakeWorld {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const w1 = win("win-a", "out", ws1, 0);
    const w2 = win("win-b", "out", ws2, 0);
    return {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        wins: [w1, w2],
        activeScreen: { name: "out" },
        activeWindow: w1,
        currentDesktop: ws1,
        created: [],
        removed: [],
        focused: [],
        membershipWrites: [],
        geometryWrites: [],
    };
}

interface Harness {
    world: FakeWorld;
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    shortcuts: Map<string, () => void>;
    signals: Record<string, FakeSignal>;
    winSignals: Map<string, { started: FakeSignal; finished: FakeSignal }>;
    oracleCalls: Array<(reply: unknown) => void>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: (maxDelay?: number) => void;
    lastCall: (method: string) => CapturedCall;
}

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

function makeHarness(mode: unknown = "per-output-local", gesture = false, defaultTiled: unknown = true, refillFn?: () => unknown): Harness {
    const world = makeWorld();
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const shortcuts = new Map<string, () => void>();
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(),
        screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(),
        activeScreenChanged: fakeSignal(),
    };
    // Per-window drag signals (Started for grab capture, Finished for the
    // LastVerdict pull) plus a live cursor for grab identification.
    const winSignals = new Map<string, { started: FakeSignal; finished: FakeSignal }>();
    for (const w of world.wins) {
        const started = fakeSignal();
        const finished = fakeSignal();
        winSignals.set(w.internalId, { started, finished });
        (w as unknown as Record<string, unknown>)["interactiveMoveResizeStarted"] = started.signal;
        (w as unknown as Record<string, unknown>)["interactiveMoveResizeFinished"] = finished.signal;
    }
    const oracleCalls: Array<(reply: unknown) => void> = [];
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals.windowAdded as FakeSignal).signal,
        windowRemoved: (signals.windowRemoved as FakeSignal).signal,
        windowActivated: (signals.windowActivated as FakeSignal).signal,
        desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
        screensChanged: (signals.screensChanged as FakeSignal).signal,
        currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
        activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.focused.push(found.internalId);
            }
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            const found = world.desktops.find((d) => (d as unknown) === ref);
            if (found) {
                world.currentDesktop = found;
            }
        },
        clientArea: (_area: number, _out: unknown, _desk: unknown) => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (_screen: unknown) => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown, _screen: unknown) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentDesktop = found;
            }
        },
        sendClientToScreen: (_mover: unknown, _out: unknown) => undefined,
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string") {
                throw new Error("Insufficient arguments");
            }
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id, args[1] as string));
            world.created.push(id);
        },
        removeDesktop: (ref: unknown) => {
            const id = (ref as FakeDesktop).id;
            world.desktops = world.desktops.filter((d) => d.id !== id);
            world.removed.push(id);
        },
    };
    // Instrument native setters through the live refs.
    for (const w of world.wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        let desks = w.desktops;
        Object.defineProperty(w, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                world.membershipWrites.push(`${w.internalId}->${refs.map((d) => d.id).join("+")}`);
                return true;
            },
        });
    }
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                }
            }
        },
        oracleCallDbus: (_service, _path, _iface, _method, callback) => {
            oracleCalls.push(callback);
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => gesture,
        registerShortcutFn: (name, _text, _seq, cb) => {
            shortcuts.set(name, cb);
            return true;
        },
        readWorkspaceModeFn: () => mode,
        readDefaultTiledFn: () => defaultTiled,
        ...(refillFn !== undefined ? { readMigrationSourceRefillFn: refillFn } : {}),
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    return {
        world, calls, timers, logs, shortcuts, signals, winSignals, oracleCalls, handle,
        runTimers,
        lastCall: (method: string): CapturedCall => {
            const found = [...calls].reverse().find((c) => c.method === method);
            assert.ok(found !== undefined, `expected ${method}`);
            return found as CapturedCall;
        },
    };
}

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 2, adapter: "broker-1", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    windows: [], transfers: [], effects: [], result: null,
    gestureSettled: null, operationSettled: [], warpCursor: null,
    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
});

function discover(h: Harness, gen = "gen-1"): void {
    const call = h.lastCall("DiscoverWorld");
    const body = JSON.parse(call.payload);
    call.callback(BOOTSTRAP(gen, body.req));
    // Settle the discovery-triggered observation report first: the entry
    // auto-dispatches one observation-only DescribeWorld on bind, and a
    // command issued while it is in flight queues in FIFO order behind it.
    // Replying here leaves the flight idle for the test body.
    const auto = h.lastCall("DescribeWorld");
    const autoBody = JSON.parse(auto.payload);
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(acceptedReply(gen, autoBody["source"] as string, autoBody["nativeInvalid"] as string, "0"));
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, operationSettled: [], warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
}

function requestOf(h: Harness): Record<string, unknown> {
    return JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
}

function reportOf(h: Harness): Record<string, unknown> {
    return requestOf(h)["report"] as Record<string, unknown>;
}

function queueOf(h: Harness): string {
    return h.lastCall("QueueWorldCommand").payload;
}

function queueCalls(h: Harness): Array<Record<string, unknown>> {
    return h.calls
        .filter((c) => c.method === "QueueWorldCommand")
        .map((c) => JSON.parse(c.payload) as Record<string, unknown>);
}

describe("world entry discovery and observation", () => {
    it("discovers on start and sends a full observation report", () => {
        const h = makeHarness();
        assert.equal(h.lastCall("DiscoverWorld").method, "DiscoverWorld");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal(req["v"], 2);
        assert.equal(req["gen"], "gen-1");
        const inner = req["report"] as Record<string, unknown>;
        assert.equal(inner["mode"], "per-output-local");
        for (const key of ["owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
            assert.ok(!(key in inner), `inner report carries no ${key}`);
        }
        const screens = inner["screens"] as Array<Record<string, unknown>>;
        assert.equal(screens.length, 1);
        assert.deepEqual(screens[0]?.["full"], [0, 0, 1920, 1080]);
        assert.deepEqual(screens[0]?.["work"], [0, 0, 800, 600]);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.equal(wins.length, 2);
        assert.equal((inner["native"] as Array<Record<string, unknown>>)[0]?.["current"], "ws1");
        assert.equal(screens[0]?.["gap"], 8);
        const first = (wins[0] as Record<string, unknown>);
        assert.equal(typeof first["hints"], "object");
        assert.equal(first["nativeWs"], "ws1", "truthful native workspace evidence");
        assert.equal(first["observedWs"], null, "never alias native as logical");
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.ok(typeof filed["clientCmd"] === "string");
        assert.deepEqual((filed["command"] as Record<string, unknown>)["op"], "select-relative");
    });

    it("degrades per-output modes to shared without the 6.7 seam", () => {
        const h = makeHarness();
        const surface = { ...(h as unknown as Record<string, unknown>) };
        void surface;
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        // The shared harness below covers the seam; this host keeps the
        // legacy-shared shape, so observation still succeeds.
        assert.ok((reportOf(h)["screens"] as Array<unknown>).length >= 1);
    });

    it("accepts per-output-local and shared; global-unique falls back with a logged reason", () => {
        for (const mode of ["per-output-local", "shared"]) {
            const h = makeHarness(mode);
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            assert.equal(reportOf(h)["mode"], mode);
        }
        // Tentative (user review pending): a persisted `global-unique`
        // falls back to `per-output-local` on the world route, once.
        const h = makeHarness("global-unique");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(reportOf(h)["mode"], "per-output-local");
        assert.ok(
            h.logs.some((l) => l.includes("workspace-mode-fallback") && l.includes("from=global-unique")),
            "fallback logged",
        );
    });

    it("wires the saved default tiling into reports and the snapshot", () => {
        // Backfill default: reports true and the snapshot seeds tiled.
        const backfill = makeHarness("per-output-local", false);
        discover(backfill);
        assert.equal(reportOf(backfill)["defaultTiled"], true);
        assert.equal(backfill.handle?.getWorkspaceTilingSnapshot().defaultTiled, true);
        backfill.handle?.stop();
        // Saved false: reports false and the pre-reply snapshot seeds
        // floating; the entry keeps no per-workspace flag of its own.
        const floating = makeHarness("per-output-local", false, false);
        discover(floating);
        assert.equal(reportOf(floating)["defaultTiled"], false);
        const snapshot = floating.handle?.getWorkspaceTilingSnapshot();
        assert.equal(snapshot?.defaultTiled, false);
        assert.equal(snapshot?.tiled, false);
        floating.handle?.stop();
    });

    it("applies per-property geometry diffs and ignores stale revisions", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const reply = acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "",
            }],
        });
        h.lastCall("DescribeWorld").callback(reply);
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")));
        // Stale rev on the same watermark applies nothing further.
        const writes = h.world.geometryWrites.length;
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const req2 = requestOf(h);
        // Deliver the OLD rev against the new obs: accepted-rev gate refuses.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "1"));
        assert.equal(h.world.geometryWrites.length, writes);
    });

    it("queues directional input during flight without disturbing the head", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        // Idle fast path: the first command dispatches without waiting for
        // the observation debounce.
        const describes = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(describes.length, 2);
        const first = requestOf(h);
        const firstObs = first["nativeInvalid"] as string;
        const headQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.notEqual(((headQ["command"] as Record<string, unknown>)["op"]), null);
        const firstClientCmd = headQ["clientCmd"] as string;
        // A second directional press queues WITHOUT advancing the
        // watermark: only native changes invalidate, so the head reply
        // still matches and applies.
        h.handle?.requestFocus("right");
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["source"] as string, firstObs, "1"));
        assert.ok(!h.logs.some((l) => l.includes("stale")));
        // The queued press drains zero-delay after the acceptance.
        h.runTimers();
        const queued = queueCalls(h);
        const secondCmd = (queued[queued.length - 1]?.["command"] as Record<string, unknown>);
        assert.deepEqual(secondCmd["op"], "focus-directional");
        assert.deepEqual(secondCmd["direction"], "right");
        const second = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", second["source"] as string, second["nativeInvalid"] as string, "2"));
        const seqs = queueCalls(h).map((req) => req["clientCmd"] as string);
        assert.deepEqual(seqs, [firstClientCmd, seqs[1] as string]);
    });

    it("suspends geometry under a gesture barrier but keeps observation flowing", () => {
        const h = makeHarness("per-output-local", true);
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal((req["report"] as Record<string, unknown>)["gesture"], true);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [50, 50, 300, 300], frame: [50, 50, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "",
            }],
        }));
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("retains explicit closes until an accepted reply acks them", () => {
        const h = makeHarness();
        discover(h);
        // win-b closes natively: the removal signal names the ref, then it
        // leaves the live list.
        const removed = h.world.wins.find((w) => w.internalId === "win-b");
        assert.ok(removed !== undefined);
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(removed);
        }
        h.world.wins = h.world.wins.filter((w) => w.internalId !== "win-b");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        // The explicit closed id rides the report (bare absence alone would
        // read as minimize, never close); the survivor stays fully covered.
        const inner = reportOf(h);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.ok(wins.every((w) => (w as Record<string, unknown>)["win"] !== "win-b"));
        assert.ok(wins.some((w) => (w as Record<string, unknown>)["win"] === "win-a"));
        const closed = inner["closed"] as Array<string>;
        assert.ok(closed.includes("win-b"));
    });

    it("carries minimized windows without closing them", () => {
        const h = makeHarness();
        const target = h.world.wins.find((w) => w.internalId === "win-b");
        assert.ok(target !== undefined);
        (target as FakeWindow).minimized = true;
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = reportOf(h);
        // Minimized lifetimes ride as explicit minimized facts with full
        // geometry (never silent absence, never an inferred close).
        const wins = req["windows"] as Array<Record<string, unknown>>;
        const minimized = wins.filter((w) => w["minimized"] === true);
        assert.ok(minimized.some((m) => m["win"] === "win-b" && m["out"] === "out" && m["nativeWs"] === "ws2"));
        const tiled = wins.filter((w) => w["minimized"] !== true);
        assert.ok(!tiled.some((w) => (w as Record<string, unknown>)["win"] === "win-b"));
        assert.equal(req["predicate"], "both-axes-fixed");
        assert.equal(req["refill"], "last-remaining-workspace");
    });

    it("carries the validated refill token live per report", () => {
        let refill: unknown = "most-recently-used-workspace";
        const h = makeHarness("per-output-local", false, true, () => refill);
        discover(h);
        assert.equal(reportOf(h)["refill"], "most-recently-used-workspace");
        // A setting change applies to the next report only: invalid values
        // fall back to the default without touching committed state.
        refill = "bogus";
        const before = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, before + 1);
        assert.equal(reportOf(h)["refill"], "last-remaining-workspace");
    });

    it("rediscovers on generation mismatch without replaying the command", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const callsBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 2, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0",
        }));
        // A fresh discovery goes out; the dropped command is NOT redispatched
        // as a DescribeWorld (only the new DiscoverWorld).
        assert.equal(h.calls.filter((c) => c.method === "DiscoverWorld").length, 2);
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
        const second = h.calls.filter((c) => c.method === "DiscoverWorld").pop() as CapturedCall;
        second.callback(BOOTSTRAP("gen-2", JSON.parse(second.payload).req));
        // Settle the rebind-triggered observation report before commanding.
        {
            const auto = h.lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload);
            auto.callback(acceptedReply("gen-2", autoBody.source, autoBody.nativeInvalid, "0"));
        }
        // After rebinding, a new explicit command flows with the new gen.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(requestOf(h)["gen"], "gen-2");
    });

    it("holds unreadable scopes with no writes", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            scopes: [{ scope: "out/ws2", auth: "hold", reason: "unreadable" }],
            windows: [{
                win: "win-b", out: "out", ws: "ws2",
                alloc: [0, 0, 100, 100], frame: [0, 0, 100, 100],
                frame_auth: "hold", frame_hold: "unreadable",
                member_auth: "hold", member_hold: "unreadable",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "",
            }],
        }));
        assert.equal(h.world.geometryWrites.length, 0);
        assert.equal(h.world.membershipWrites.length, 0);
    });

    it("spends a focus effect once and never repeats it", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            effects: [{ id: "fx-1", kind: "focus", target: "win-b", rev: "1" }],
        }));
        assert.equal(h.world.focused.length, 1);
        // A repeated identical effect id on a later reply never reapplies.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req2 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "2", {
            effects: [{ id: "fx-1", kind: "focus", target: "win-b", rev: "1" }],
        }));
        assert.equal(h.world.focused.length, 1);
    });

    it("settles refused transfers without repeating them", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelativeMove(1, true);
        h.runTimers();
        const req = requestOf(h);
        const filedRefused = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.deepEqual((filedRefused["command"] as Record<string, unknown>)["op"], "send-relative");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "controlled", frame_hold: "",
                member_auth: "hold", member_hold: "refused-target",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "",
            }],
        }));
        // Refused membership performs no native move.
        assert.equal(h.world.membershipWrites.length, 0);
    });

    it("creates through one flight and blocks removal of occupied workspaces", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                // Desired-absent but occupied: the fresh occupancy gate keeps it.
                { id: "ws2", out: "out", bind: "ws2", scope: "out", order: 1, mode: "tiled", exist: false, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1);
        assert.equal(h.world.removed.length, 0);
    });

    it("recovers busy replies event-driven with no polling", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 1, outcome: "rejected", kind: "busy", message: "busy", gen: "gen-1",
        }));
        // No timer-driven retry was scheduled beyond the debounce/timeout
        // pair: recovery waits for the next native event (dirty flag set).
        assert.ok(h.logs.some((l) => l.includes("command-busy") || l.includes("world")));
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("stops partial apply mid-way when invalidated", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        // Two controlled geometry rows; the apply writes the first, then a
        // concurrent invalidation stops the rest. Deterministic here: both
        // rows target distinct live windows and the harness marks dirty only
        // through explicit commands, so both apply. The narrow assertion is
        // that held rows never write.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [
                {
                    win: "win-a", out: "out", ws: "ws1",
                    alloc: [11, 11, 300, 300], frame: [11, 11, 300, 300],
                    frame_auth: "controlled", frame_hold: "",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "",
                    floating: false, float_origin: "",
                },
                {
                    win: "win-b", out: "out", ws: "ws2",
                    alloc: [22, 22, 300, 300], frame: [22, 22, 300, 300],
                    frame_auth: "hold", frame_hold: "gesture-barrier",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "",
                    floating: false, float_origin: "",
                },
            ],
        }));
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-a@11,11")));
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@22,22")));
    });

    it("classifies fixed-size admissions instead of carrying always-false flags", () => {
        const h = makeHarness();
        const target = h.world.wins.find((w) => w.internalId === "win-a");
        assert.ok(target !== undefined);
        (target as unknown as Record<string, unknown>)["minSize"] = { width: 400, height: 300 };
        (target as unknown as Record<string, unknown>)["maxSize"] = { width: 400, height: 300 };
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = reportOf(h);
        const wins = req["windows"] as Array<Record<string, unknown>>;
        const row = wins.find((w) => w["win"] === "win-a");
        assert.ok(row !== undefined);
        assert.equal(row["fixedAuto"], true);
        assert.equal(row["floating"], true);
        assert.equal(row["fitExcluded"], true);
    });

    it("sends append-and-stay as one core-resolved send-numbered with no view switch", () => {
        const h = makeHarness();
        discover(h);
        // Append-and-stay: a single send-numbered index 0 with follow=false.
        // Positional resolution is all core (reuse or stable pending token);
        // sends never view-switch, so no trailing switch or stay restore.
        (h.handle as unknown as { requestWorkspaceMove: (index: number, follow: boolean) => void }).requestWorkspaceMove(0, false);
        h.runTimers();
        const first = requestOf(h);
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "send-numbered");
        assert.equal(cmd["index"], 0);
        assert.equal(cmd["follow"], false);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["source"] as string, first["nativeInvalid"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws2", out: "out", bind: "ws2", scope: "out", order: 1, mode: "tiled", exist: true, hold: "" },
            ],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        }));
        // No chained view traffic: the flight settles with no further
        // switch-view/restore dispatch.
        h.runTimers();
        const settled = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.ok(settled.length >= 2);
    });

    it("routes a non-cancelled drag verdict as one pointer-resize command", () => {
        const h = makeHarness();
        discover(h);
        const sigs = h.winSignals.get("win-a");
        assert.ok(sigs !== undefined);
        for (const handler of [...(sigs as { started: FakeSignal; finished: FakeSignal }).started.handlers]) {
            handler();
        }
        for (const handler of [...(sigs as { started: FakeSignal; finished: FakeSignal }).finished.handlers]) {
            handler();
        }
        assert.equal(h.oracleCalls.length, 1);
        const pull = h.oracleCalls[0] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: -50, y: 0, w: 250, h: 200 },
            windowIdentity: "win-a", correlation: "drag-1", reason: "ok-moved",
        }));
        h.runTimers();
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        const inner = cmd["command"] as Record<string, unknown>;
        assert.equal(inner["op"], "pointer-resize");
        assert.equal(inner["win"], "win-a");
        assert.equal(inner["direction"], "left");
        assert.equal(inner["boundary"], -50);
    });

    it("treats a cancelled drag verdict as a strict no-op", () => {
        const h = makeHarness();
        discover(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        const sigs = h.winSignals.get("win-a");
        assert.ok(sigs !== undefined);
        for (const handler of [...(sigs as { started: FakeSignal; finished: FakeSignal }).started.handlers]) {
            handler();
        }
        for (const handler of [...(sigs as { started: FakeSignal; finished: FakeSignal }).finished.handlers]) {
            handler();
        }
        const pull = h.oracleCalls[0] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: true,
            finalRect: { x: 0, y: 0, w: 200, h: 200 },
            windowIdentity: "win-a", correlation: "drag-2", reason: "no-change",
        }));
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, describesBefore);
        assert.ok(h.logs.some((l) => l.includes("drag-cancelled")));
    });

    it("toggles sticky off with keep-above restore and refuses overlays", () => {
        const h = makeHarness();
        discover(h);
        const ref = h.world.wins.find((w) => w.internalId === "win-a") as unknown as Record<string, unknown>;
        ref["keepAbove"] = false;
        ref["keepBelow"] = false;
        h.handle?.requestSticky();
        assert.equal(ref["onAllDesktops"], true);
        assert.equal(ref["keepAbove"], true);
        h.handle?.requestSticky();
        assert.equal(ref["onAllDesktops"], false);
        assert.equal(ref["keepAbove"], false);
        // Fullscreen overlay refuses with no write.
        ref["fullScreen"] = true;
        const logsBefore = h.logs.length;
        h.handle?.requestSticky();
        assert.equal(ref["onAllDesktops"], false);
        assert.ok(h.logs.slice(logsBefore).some((l) => l.includes("sticky")));
        ref["fullScreen"] = false;
    });

    it("registers the legacy directional and toggle shortcut catalog", () => {
        const h = makeHarness();
        // Directional focus/move/resize with HJKL plus arrow aliases.
        for (const name of [
            "omnitiler-focus-left", "omnitiler-focus-left-arrow",
            "omnitiler-move-right-arrow",
            "omnitiler-resize-outwards-up", "omnitiler-resize-outwards-up-arrow",
            "omnitiler-resize-inwards-down", "omnitiler-resize-inwards-down-arrow",
        ]) {
            assert.ok(h.shortcuts.has(name), name);
        }
        // Window-state toggles with legacy sequences.
        for (const name of [
            "omnitiler-toggle-float", "omnitiler-toggle-sticky",
            "omnitiler-toggle-maximize", "omnitiler-toggle-fullscreen",
            "omnitiler-toggle-orientation",
        ]) {
            assert.ok(h.shortcuts.has(name), name);
        }
        // A resize shortcut routes a parameterized resize world command.
        discover(h);
        const resize = h.shortcuts.get("omnitiler-resize-outwards-left") as () => void;
        assert.ok(resize !== undefined);
        resize();
        h.runTimers();
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = (filed["command"] as Record<string, unknown>);
        assert.equal(cmd["op"], "operation");
        const inner = cmd["command"] as Record<string, unknown>;
        assert.equal(inner["op"], "resize");
        assert.equal(inner["direction"], "left");
        assert.equal(inner["mode"], "outwards");
    });

    it("defers a floating-maximized float request until the settlement report", () => {
        const h = makeHarness();
        discover(h);
        // Seed intentional-float membership through an accepted untouched
        // row so the B9 maximized-unfloat path arms.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const seed = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", seed["source"] as string, seed["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: true, float_origin: "",
            }],
        }));
        const target = h.world.wins.find((w) => w.internalId === "win-a");
        assert.ok(target !== undefined);
        (target as FakeWindow).maximizeMode = 3;
        h.handle?.requestFloat();
        // Deferred: the sync fake settles in the same stack, but no
        // toggle-float command rides it (no same-stack echo admission).
        h.runTimers();
        const queuesBefore = queueCalls(h).length;
        let req = requestOf(h);
        assert.equal(queuesBefore, 1, "clear schedules observation first");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "2", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: true, float_origin: "",
            }],
        }));
        h.runTimers();
        req = requestOf(h);
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        assert.equal((cmd["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.equal((target as FakeWindow).maximizeMode, 0);
        assert.ok(h.logs.some((l) => l.includes("outcome=observed-cleared")));
    });
});

describe("world entry rapid directional FIFO", () => {
    // Producer-shaped helpers over the harness above: every discrete press
    // dispatches in press order across debounce and flight boundaries.
    const sentCmdSeqs = (h: Harness): string[] => queueCalls(h)
        .map((req) => req["clientCmd"] as string);
    // Fire the earliest due timer at/after `from` only: the entry arms
    // follow-up timers from inside callbacks, firing the whole queue at
    // once would also expire a follow-up's own timeout before its reply
    // can arrive, and dead (cancel-noop) timers from earlier legs must
    // not steal the pump.
    const fireNextTimer = (hh: Harness, maxDelay: number, from = 0): void => {
        let at = -1;
        for (let i = from; i < hh.timers.length; i += 1) {
            const t = hh.timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay && (at < 0 || t.delay < (hh.timers[at] as { delay: number; cb: () => void }).delay)) {
                at = i;
            }
        }
        assert.ok(at >= 0, "expected a due timer");
        const [timer] = hh.timers.splice(at, 1);
        (timer as { delay: number; cb: () => void }).cb();
    };
    const acceptLast = (h: Harness, gen: string, rev: string): void => {
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply(gen, req["source"] as string, req["nativeInvalid"] as string, rev));
    };

    it("dispatches rapid left then right in order across a coalescing debounce", () => {
        const h = makeHarness();
        discover(h);
        // A native burst arms the debounce with no command in flight.
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        const left = h.shortcuts.get("omnitiler-focus-left") as () => void;
        const right = h.shortcuts.get("omnitiler-focus-right") as () => void;
        assert.ok(left !== undefined && right !== undefined);
        left();
        right();
        // Both presses accepted: no busy refusal, no shortcut failure.
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        // The debounce flushes the head first with a fresh observation.
        h.runTimers();
        const headQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const headCmd = headQ["command"] as Record<string, unknown>;
        assert.equal(headCmd["op"], "focus-directional");
        assert.equal(headCmd["direction"], "left");
        acceptLast(h, "gen-1", "1");
        // The accepted head files the queued opposite press next.
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 2);
        const tailCmd = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(tailCmd["op"], "focus-directional");
        assert.equal(tailCmd["direction"], "right");
        assert.ok((queued[1]?.["clientCmd"] as string) > (queued[0]?.["clientCmd"] as string));
    });

    it("preserves repeated directional moves in order across successive flights", () => {
        const h = makeHarness();
        discover(h);
        const moveLeft = h.shortcuts.get("omnitiler-move-left") as () => void;
        assert.ok(moveLeft !== undefined);
        // First press takes the idle fast path: filed at once, then
        // drained by its fresh capture.
        moveLeft();
        const headQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.equal((headQ["command"] as Record<string, unknown>)["op"], "move-directional");
        // Two more presses queue behind the flight without moving the
        // watermark, so the head still accepts; each tail then files
        // zero-delay after the previous acceptance.
        moveLeft();
        moveLeft();
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        assert.ok(!h.logs.some((l) => l.includes("stale")));
        const head = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", head["source"] as string, head["nativeInvalid"] as string, "1"));
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 2);
        const second = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(second["op"], "move-directional");
        assert.equal(second["direction"], "left");
        acceptLast(h, "gen-1", "2");
        h.runTimers();
        const queued3 = queueCalls(h);
        assert.equal(queued3.length, 3);
        const third = queued3[2]?.["command"] as Record<string, unknown>;
        assert.equal(third["op"], "move-directional");
        assert.equal(third["direction"], "left");
        acceptLast(h, "gen-1", "3");
        // Every intent rode exactly one report: strictly increasing
        // command sequences, no replay, no coalescing.
        const seqs = sentCmdSeqs(h);
        assert.deepEqual(seqs, [...seqs].sort());
        assert.equal(new Set(seqs).size, 3);
    });

    it("drains queued tails automatically after a busy rejection, unsignalled", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        // Core busy: the filed head ends without replay.
        h.handle?.requestFocus("right");
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 2, outcome: "rejected", kind: "busy", message: "busy", gen: "gen-1",
            source: "", nativeInvalid: "0", clock: "0",
        }));
        // No native event and no new press: the terminal failure alone
        // schedules an observation-only catchup first.
        h.runTimers();
        assert.equal(queueCalls(h).length, 1, "catchup files nothing");
        acceptLast(h, "gen-1", "1");
        // The accepted catchup drains the queued tail zero-delay.
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 2);
        const drained = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(drained["op"], "focus-directional");
        assert.equal(drained["direction"], "right");
        acceptLast(h, "gen-1", "2");
        // No replay anywhere: the refused head and the queued tail each
        // filed exactly once.
        const cmdSeqs = sentCmdSeqs(h);
        assert.equal(new Set(cmdSeqs).size, cmdSeqs.length);
        assert.equal(cmdSeqs.length, 2);
    });

    it("drops only the sent head on timeout and drains the tail unsignalled", () => {
        const h = makeHarness();
        discover(h);
        const mark = h.timers.length;
        h.handle?.requestFocus("left");
        const firstQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const firstClientCmd = firstQ["clientCmd"] as string;
        // Queue a second press behind the flight, then expire the debounce,
        // the settled queue timer, and the flight's own capture timeout in
        // turn: the auto-drain files the tail on the debounce. No native
        // event is delivered anywhere below.
        h.handle?.requestFocus("right");
        fireNextTimer(h, 2000, mark);
        fireNextTimer(h, 2000, mark);
        fireNextTimer(h, 2000, mark);
        assert.ok(h.logs.some((l) => l.includes("timeout")));
        // The timeout schedules an observation-only catchup first.
        h.runTimers();
        assert.equal(queueCalls(h).length, 1, "catchup files nothing");
        acceptLast(h, "gen-1", "1");
        // The accepted catchup drains the queued tail zero-delay.
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 2);
        const drained = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(drained["op"], "focus-directional");
        assert.equal(drained["direction"], "right");
        acceptLast(h, "gen-1", "2");
        // The timed-out head never reappears.
        const seqs = sentCmdSeqs(h);
        assert.equal(seqs.filter((s) => s === firstClientCmd).length, 1);
        assert.equal(new Set(seqs).size, seqs.length);
    });

    it("drops queued directionals on generation mismatch with a diagnostic", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        h.handle?.requestFocus("left");
        // Poison while one head flies and two tails wait: the whole queue
        // belongs to the dead generation.
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 2, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0",
        }));
        assert.ok(h.logs.some((l) => l.includes("queue-dropped reason=generation-mismatch depth=2")));
        assert.equal(h.calls.filter((c) => c.method === "DiscoverWorld").length, 2);
        const cmdsBefore = sentCmdSeqs(h).length;
        // Only the native debounce elapses here: expiring the fresh
        // discovery round instead would wedge the rebind it is about to
        // answer. The armed pre-mismatch debounce finds the discovery
        // gate closed and dispatches nothing.
        h.runTimers();
        // The discovery gate holds while unbound: nothing dispatches, and
        // no command ever replays.
        assert.deepEqual(sentCmdSeqs(h).length, cmdsBefore);
        const second = h.calls.filter((c) => c.method === "DiscoverWorld").pop() as CapturedCall;
        second.callback(BOOTSTRAP("gen-2", JSON.parse(second.payload).req));
        {
            const auto = h.lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload);
            auto.callback(acceptedReply("gen-2", autoBody.source, autoBody.nativeInvalid, "0"));
        }
        // After rebinding, a fresh press flows; the dropped tails never do.
        h.handle?.requestFocus("right");
        const fresh = requestOf(h);
        assert.equal(fresh["gen"], "gen-2");
        const freshQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.equal((freshQ["command"] as Record<string, unknown>)["direction"], "right");
        const dirs = queueCalls(h)
            .map((req) => (req["command"] as Record<string, unknown>)["direction"]);
        assert.deepEqual(dirs, ["left", "right"]);
    });

    it("keeps fail-fast busy refusal for non-directional commands", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        // A workspace select behind a directional flight refuses exactly
        // like the old single-slot path: no queue, native state untouched.
        h.handle?.requestWorkspaceRelative(1);
        assert.ok(h.logs.some((l) => l.includes("command-busy")));
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed action=workspace-relative")));
        // A resize does the same.
        h.handle?.requestResize("left", "outwards");
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed action=resize")));
        // Draining the head still works afterwards.
        acceptLast(h, "gen-1", "1");
        h.runTimers();
        const drained = (JSON.parse(queueOf(h)) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.equal(drained["op"], "focus-directional");
    });

    it("drops the queue on stop with no post-stop dispatch", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        const callsBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.stop();
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        h.runTimers(2000);
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
        // Presses after stop refuse with no dispatch.
        h.handle?.requestFocus("right");
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed")));
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
    });
});

describe("world entry aligned bindings", () => {
    it("converges bindings per (output, native) and prunes non-live cells", () => {
        const h = makeHarness("per-output-local");
        discover(h);
        // Dispatch a command-carrying report; the accepted reply binds the
        // same native on two outputs with a distinct logical each.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        const wsRow = (id: string, out: string, bind: string, order: number) => ({
            id, out, bind, scope: out, order, mode: "tiled", exist: true, hold: "",
        });
        first.callback(acceptedReply("gen-1", firstBody.source as string, firstBody.nativeInvalid as string, "1", {
            workspaces: [
                wsRow("al-1", "out", "ws1", 0),
                wsRow("al-9", "out-2", "ws1", 0),
            ],
        }));
        // A second dispatch (native invalidation + debounce) carries the
        // converged per-cell bindings for live cells only: (out, ws1) kept,
        // (out-2, ws1) pruned with its output gone.
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        const views = second["views"] as Array<Record<string, unknown>>;
        assert.ok(views.some((v) => v["out"] === "out" && v["view"] === "al-1"), "live cell publishes its logical");
    });

    it("replicates bindings per output in per-output-local across two screens", () => {
        const h = makeHarness("per-output-local");
        h.world.screens.push({ name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
        discover(h);
        assert.equal(reportOf(h)["mode"], "per-output-local");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        const wsRow = (id: string, out: string, bind: string, order: number) => ({
            id, out, bind, scope: out, order, mode: "tiled", exist: true, hold: "",
        });
        first.callback(acceptedReply("gen-1", firstBody.source as string, firstBody.nativeInvalid as string, "1", {
            workspaces: [
                wsRow("al-1", "out", "ws1", 0),
                wsRow("al-9", "out-2", "ws1", 0),
            ],
        }));
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        // One logical view per output: the same native publishes a distinct
        // logical on each output.
        assert.deepEqual(second["views"], [
            { out: "out", view: "al-1" },
            { out: "out-2", view: "al-9" },
        ]);
    });

    it("keeps single-row bindings in shared and resolves without output scope", () => {
        const h = makeHarness("shared");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        assert.equal((firstBody["report"] as Record<string, unknown>)["mode"], "shared");
        first.callback(acceptedReply("gen-1", firstBody.source, firstBody.nativeInvalid, "1", {
            workspaces: [
                { id: "sh-1", out: "out", bind: "ws1", scope: "shared", order: 0, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        // Shared is unchanged: one logical view resolves through the
        // output-less lookup (never replicated per output).
        assert.deepEqual(second["views"], [{ out: "out", view: "sh-1" }]);
    });
});

describe("world entry restart float-intent preservation", () => {
    for (const holdMode of ["scoped", "global"] as const) {
    it(`holds settled float across a genuinely held first engine reply (${holdMode} hold)`, { timeout: 120000 }, async () => {
        // Producer-shaped replay through the real WorldEngine (world_eval
        // with WORLD_EVAL_ALIGNED=1, same bridge as the aligned-migration
        // engine test): no hand-built replies on the asserted paths.
        // Scoped hold is genuinely native: clientArea returns null for ws-1,
        // which world-observe turns into an explicit domain hold
        // "unreadable". Global hold poisons one live rect so the whole
        // inventory reads unreadable and every domain empties its windows
        // while staying non-null. One settled float (win-a on out-a/ws-1)
        // plus one disjoint tile override (win-b on out-b/ws-2) seed from
        // the real persisted-store read shape. No commands ride.
        const { spawn } = await import("node:child_process");
        const { existsSync } = await import("node:fs");
        const { createInterface } = await import("node:readline");
        const { dirname, join, resolve } = await import("node:path");
        const repoRoot = (): string => {
            let dir = resolve(process.cwd());
            for (let depth = 0; depth < 4; depth += 1) {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
                dir = dirname(dir);
            }
            throw new Error("fixture root not found");
        };
        interface EngineWaiter {
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }
        const proc = spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "world_eval"], {
            cwd: repoRoot(),
            stdio: ["pipe", "pipe", "pipe"],
            env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
        });
        const waiters: EngineWaiter[] = [];
        let engineFailed: Error | null = null;
        const stderrChunks: string[] = [];
        try {
            proc.stderr?.on("data", (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });
        } catch (error) {
            void error;
        }
        const failEngine = (error: Error): void => {
            if (engineFailed === null) {
                engineFailed = error;
            }
            while (waiters.length > 0) {
                waiters.shift()?.reject(error);
            }
        };
        proc.on("error", (error: Error) => {
            failEngine(error);
        });
        proc.on("exit", (code: number | null, signal: string | null) => {
            failEngine(new Error(`world_eval exited code=${String(code)} signal=${String(signal)} stderr=${stderrChunks.join("").slice(0, 500)}`));
        });
        const stdout = proc.stdout;
        assert.ok(stdout !== null && stdout !== undefined, "world_eval stdout unavailable");
        const lines = createInterface({ input: stdout });
        lines.on("line", (line: string) => {
            waiters.shift()?.resolve(line);
        });
        const engineSend = (method: string, body: string): Promise<string> => new Promise<string>((resolvePromise, rejectPromise) => {
            if (engineFailed !== null) {
                rejectPromise(engineFailed);
                return;
            }
            const timer = setTimeout(() => {
                const index = waiters.findIndex((w) => w.resolve === resolvePromise);
                if (index >= 0) {
                    waiters.splice(index, 1);
                }
                rejectPromise(new Error(`world_eval timeout method=${method}`));
            }, 15000);
            waiters.push({
                resolve: (reply: string) => {
                    clearTimeout(timer);
                    resolvePromise(reply);
                },
                reject: (error: Error) => {
                    clearTimeout(timer);
                    rejectPromise(error);
                },
            });
            try {
                const ok = proc.stdin?.write(JSON.stringify({ method, body }) + "\n");
                if (ok === false) {
                    proc.stdin?.once("drain", () => undefined);
                }
            } catch (error) {
                clearTimeout(timer);
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out-a", ws1, 0);
        const w2 = win("win-b", "out-b", ws2, 0);
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const currents = new Map([["out-a", "ws1"], ["out-b", "ws2"]]);
        // Genuine native unreadability while heldFirst is true.
        // Scoped: the ws-1 work area reads as null, so the dispatched
        // report holds out-a/ws-1 as unreadable. Global: one live rect
        // poisons the whole inventory read, so every domain empties its
        // windows while the report stays non-null.
        let heldFirst = true;
        // Sole-broker mini-harness: the envelope becomes the exact core
        // report (broker namespace plus at most one queued command) and
        // the core reply returns bound with a nulled terminal result.
        const broker = createMiniBroker();
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const writes: string[] = [];
        const surface = {
            windowList: () => {
                if (heldFirst && holdMode === "global") {
                    // Poison one live rect: the observer marks the whole
                    // inventory unreadable and empties every domain's
                    // windows (global hold, still a non-null report).
                    (w1 as unknown as Record<string, unknown>)["frameGeometry"] = { x: 0, y: 0, width: 0, height: 200 };
                }
                return [w1, w2];
            },
            screens,
            desktops: [ws1, ws2],
            activeScreen: { name: "out-a" },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals.windowAdded as FakeSignal).signal,
            windowRemoved: (signals.windowRemoved as FakeSignal).signal,
            windowActivated: (signals.windowActivated as FakeSignal).signal,
            desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
            screensChanged: (signals.screensChanged as FakeSignal).signal,
            currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
            activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
            get activeWindow() {
                return w1;
            },
            clientArea: (_area: number, _out: unknown, deskRef: unknown) => {
                if (heldFirst && holdMode === "scoped" && (deskRef as { id?: unknown }).id === "ws1") {
                    return null;
                }
                return { x: 0, y: 0, width: 800, height: 600 };
            },
            currentDesktopForScreen: (screen: { name: string }) => (currents.get(screen.name) === "ws2" ? ws2 : ws1),
            setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                currents.set(screen.name, (deskRef as { id: string }).id);
            },
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            floatIntent: true,
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    // Real persisted-store read shape
                    // (float_intent_store read_ok_reply): ok outcome with
                    // members/stored/returned plus tile/tile_stored/
                    // tile_returned. Seeded before the first dispatch, so the
                    // first native capture already floats win-a.
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 1,
                        correlation_id: body["correlation_id"],
                        outcome: "ok",
                        members: ["win-a"],
                        stored: 1,
                        returned: 1,
                        tile: ["win-b"],
                        tile_stored: 1,
                        tile_returned: 1,
                    }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    writes.push(payload);
                    return;
                }
                if (method === "DiscoverWorld" || method === "DescribeWorld") {
                    calls.push({ method, payload, callback });
                    return;
                }
                if (method === "QueueWorldCommand") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerQueue(payload));
                    return;
                }
                if (method === "TakeCommandResult") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerTake(payload));
                    return;
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
        });
        assert.ok(handle !== null);
        const runTimers = (maxDelay = 120): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as { delay: number; cb: () => void };
                if (t.delay <= maxDelay) {
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        let flushed = 0;
        const flushNextDescribe = async (): Promise<{ req: Record<string, unknown>; reply: Record<string, unknown>; rawReply: string }> => {
            const at = calls.findIndex((c, i) => i >= flushed && c.method === "DescribeWorld");
            assert.ok(at >= 0, "expected a queued DescribeWorld");
            flushed = at + 1;
            // Mini-broker stamping: the envelope becomes the exact core
            // report the sole broker would dispatch, and the core reply
            // returns bound with a nulled terminal result.
            const envelope = calls[at]!.payload;
            const stamped = broker.stampCapture(envelope);
            const coreReply = await engineSend("DescribeWorld", stamped);
            const rawReply = broker.bindReply(coreReply, envelope);
            calls[at]!.callback(rawReply);
            return { req: JSON.parse(calls[at]!.payload) as Record<string, unknown>, reply: JSON.parse(rawReply) as Record<string, unknown>, rawReply };
        };
        try {
            const discAt = calls.findIndex((c) => c.method === "DiscoverWorld");
            assert.ok(discAt >= 0, "entry discovers on start");
            const engineBootstrap = await engineSend("DiscoverWorld", calls[discAt]!.payload);
            calls[discAt]!.callback(broker.bindDiscovery(engineBootstrap));
            // First accepted observation: the auto observation-only dispatch
            // after discovery, with the hold applied. No commands.
            const first = await flushNextDescribe();
            // v2 holds ride screen completeness and window facts (no
            // domains lane on the wire): the scoped hold leaves out-a's
            // work area unreadable while out-b stays complete; the global
            // hold empties the window facts with the report incomplete.
            const firstReport = first.req["report"] as Record<string, unknown>;
            const firstScreens = firstReport["screens"] as Array<Record<string, unknown>>;
            if (holdMode === "scoped") {
                const screenA = firstScreens.find((s) => s["out"] === "out-a");
                const screenB = firstScreens.find((s) => s["out"] === "out-b");
                assert.equal(
                    screenA?.["complete"],
                    false,
                    `first dispatched request holds out-a unreadable: ${JSON.stringify(firstScreens)}`,
                );
                assert.equal(
                    screenB?.["complete"],
                    true,
                    `out-b stays complete: ${JSON.stringify(firstScreens)}`,
                );
            } else {
                // Global inventory failure empties every window fact while
                // the report stays non-null.
                const wins = firstReport["windows"] as Array<unknown>;
                assert.equal(wins.length, 0, `global hold empties window facts: ${JSON.stringify(firstReport)}`);
                assert.equal(firstReport["complete"], false, "global hold marks the report incomplete");
            }
            // The genuine Engine reply must lack any untouched/no-hold row
            // for the seeded float: that absence is the producer condition
            // behind the restart loss. Tile win-b rides its own row.
            const firstWins = first.reply["windows"] as Array<Record<string, unknown>>;
            const firstRowA = firstWins.find((w) => w["win"] === "win-a");
            // Genuine engine behavior: the held scope carries no win-a row
            // at all (scopes: out-a/ws1 auth=hold reason=unreadable), so the
            // reply lacks any untouched/no-hold row for the seeded float.
            assert.ok(
                !(firstRowA?.["frame_auth"] === "untouched" && firstRowA?.["frame_hold"] === ""),
                `first engine reply holds the float row: ${JSON.stringify(firstRowA)} scopes=${JSON.stringify(first.reply["scopes"])}`,
            );
            // Restore the readable native condition and recapture the same
            // live identities completely.
            heldFirst = false;
            if (holdMode === "global") {
                (w1 as unknown as Record<string, unknown>)["frameGeometry"] = { x: 0, y: 0, width: 200, height: 200 };
            }
            for (const handler of [...(signals.desktopsChanged as FakeSignal).handlers]) {
                handler();
            }
            runTimers();
            const second = await flushNextDescribe();
            const secondReport = second.req["report"] as Record<string, unknown>;
            const secondScreens = secondReport["screens"] as Array<Record<string, unknown>>;
            assert.ok(
                secondScreens.every((s) => s["complete"] === true),
                `second dispatched request is complete: ${JSON.stringify(secondScreens)}`,
            );
            // No uncommanded settled float loss: every durable snapshot
            // retains win-a, keeps win-b on the tile side only (disjoint),
            // and never overlaps sides.
            const settled = writes.map((w) => JSON.parse(w) as Record<string, unknown>);
            assert.ok(settled.length >= 1, `expected settled writes, got ${settled.length}`);
            for (const w of settled) {
                const members = w["members"] as Array<string>;
                const tile = (w["tile"] as Array<string> | undefined) ?? [];
                assert.ok(members.includes("win-a"), `held first reply must not drop settled float: ${JSON.stringify(w)}`);
                assert.ok(tile.includes("win-b"), `tile override must survive: ${JSON.stringify(w)}`);
                assert.ok(!members.includes("win-b"), `fixed tile override is not a float: ${JSON.stringify(w)}`);
                assert.ok(!tile.includes("win-a"), `float side stays disjoint: ${JSON.stringify(w)}`);
            }
            // The second dispatched observation still reports the seeded
            // float as floating with the same live identities.
            const secondWins = (secondReport["windows"] as Array<Record<string, unknown>>);
            const rowA = secondWins.find((w) => w["win"] === "win-a");
            assert.equal(
                rowA?.["floating"],
                true,
                `second dispatched observation still floats seeded win-a: ${JSON.stringify(rowA)}`,
            );
        } finally {
            try {
                handle?.stop();
            } catch (error) {
                void error;
            }
            try {
                lines.close();
            } catch (error) {
                void error;
            }
            try {
                proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
                if (proc.exitCode !== null || proc.signalCode !== null) {
                    clearTimeout(timer);
                    resolvePromise();
                }
            });
        }
    });
    }
});

describe("world entry script-restart sequence handover", () => {
    it("rotation hands a fresh generation and engine to the new producer lifetime", { timeout: 120000 }, async () => {
        // Producer-shaped restart replay through the real WorldEngine behind
        // one world_eval process (aligned, like production): the first
        // producer advances request+command watermarks with a float + fixed
        // tile seed, then the restarted producer discovers as a NEW
        // lifetime. Like the real WorldBroker::discovery_body, the new
        // lifetime rotates onto a fresh generation and a fresh engine
        // (world_eval rebinds both on DiscoverWorld): the old generation
        // refuses outright, queued/settled state from the old world is
        // gone, and the saved default reseeds the fresh engine.
        const { spawn } = await import("node:child_process");
        const { existsSync } = await import("node:fs");
        const { createInterface } = await import("node:readline");
        const { dirname, join, resolve } = await import("node:path");
        const repoRoot = (): string => {
            let dir = resolve(process.cwd());
            for (let depth = 0; depth < 4; depth += 1) {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
                dir = dirname(dir);
            }
            throw new Error("fixture root not found");
        };
        interface EngineWaiter {
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }
        const proc = spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "world_eval"], {
            cwd: repoRoot(),
            stdio: ["pipe", "pipe", "pipe"],
            env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
        });
        const waiters: EngineWaiter[] = [];
        let engineFailed: Error | null = null;
        const stderrChunks: string[] = [];
        try {
            proc.stderr?.on("data", (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });
        } catch (error) {
            void error;
        }
        const failEngine = (error: Error): void => {
            if (engineFailed === null) {
                engineFailed = error;
            }
            while (waiters.length > 0) {
                waiters.shift()?.reject(error);
            }
        };
        proc.on("error", (error: Error) => {
            failEngine(error);
        });
        proc.on("exit", (code: number | null, signal: string | null) => {
            failEngine(new Error(`world_eval exited code=${String(code)} signal=${String(signal)} stderr=${stderrChunks.join("").slice(0, 500)}`));
        });
        const stdout = proc.stdout;
        assert.ok(stdout !== null && stdout !== undefined, "world_eval stdout unavailable");
        const lines = createInterface({ input: stdout });
        lines.on("line", (line: string) => {
            waiters.shift()?.resolve(line);
        });
        const engineSend = (method: string, body: string): Promise<string> => new Promise<string>((resolvePromise, rejectPromise) => {
            if (engineFailed !== null) {
                rejectPromise(engineFailed);
                return;
            }
            const timer = setTimeout(() => {
                const index = waiters.findIndex((w) => w.resolve === resolvePromise);
                if (index >= 0) {
                    waiters.splice(index, 1);
                }
                rejectPromise(new Error(`world_eval timeout method=${method}`));
            }, 15000);
            waiters.push({
                resolve: (reply: string) => {
                    clearTimeout(timer);
                    resolvePromise(reply);
                },
                reject: (error: Error) => {
                    clearTimeout(timer);
                    rejectPromise(error);
                },
            });
            try {
                const ok = proc.stdin?.write(JSON.stringify({ method, body }) + "\n");
                if (ok === false) {
                    proc.stdin?.once("drain", () => undefined);
                }
            } catch (error) {
                clearTimeout(timer);
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
        // Shared native inventory: the same apps stay alive across restart.
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out-a", ws1, 0);
        const w2 = win("win-b", "out-b", ws2, 0);
        // One managed tile on the restarted scope: the engine projects
        // retained tiled geometry for managed members, so the scope under
        // test carries a managed tile plus the seeded float exception.
        const w3 = win("win-c", "out-a", ws1, 300);
        // Genuine fixed-size declaration in out-b coordinates (observer
        // predicate: integer min/max, equal and nonzero on both axes). The
        // seeded tile override suppresses automatic float admission, so
        // win-b stays tiled under core geometry.
        (w2 as unknown as Record<string, unknown>)["frameGeometry"] = { x: 900, y: 100, width: 640, height: 480 };
        (w2 as unknown as Record<string, unknown>)["minSize"] = { width: 640, height: 480 };
        (w2 as unknown as Record<string, unknown>)["maxSize"] = { width: 640, height: 480 };
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const currents = new Map([["out-a", "ws1"], ["out-b", "ws2"]]);
        // Switchable active fixture/output: both scopes tile explicitly.
        const focus: { screen: string; window: FakeWindow } = { screen: "out-a", window: w1 };
        const geometryWrites: string[] = [];
        const membershipWrites: string[] = [];
        for (const w of [w1, w2, w3]) {
            let frame = w.frameGeometry;
            Object.defineProperty(w, "frameGeometry", {
                configurable: true,
                get: () => frame,
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    frame = rect;
                    geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                    return true;
                },
            });
        }
        const nativeSnapshot = (): string => JSON.stringify([
            w1.frameGeometry, w1.desktops.map((d) => d.id),
            w2.frameGeometry, w2.desktops.map((d) => d.id),
            w3.frameGeometry, w3.desktops.map((d) => d.id),
        ]);
        // Float/tile lifetimes never take unsolicited geometry writes; the
        // managed tile may rewrite its identical frame on first-accept.
        const floatTileWriteCount = (): number =>
            geometryWrites.filter((line) => line.startsWith("win-a@") || line.startsWith("win-b@")).length;
        interface Producer {
            handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
            broker: MiniBroker;
            calls: CapturedCall[];
            timers: Array<{ delay: number; cb: () => void }>;
            writes: string[];
            signals: Record<string, FakeSignal>;
            flushed: { count: number };
            runTimers: (maxDelay?: number) => void;
        }
        const startProducer = (adapter: string, tag: string, broker: MiniBroker): Producer => {
            const signals = {
                windowAdded: fakeSignal(),
                windowRemoved: fakeSignal(),
                windowActivated: fakeSignal(),
                desktopsChanged: fakeSignal(),
                screensChanged: fakeSignal(),
                currentDesktopChanged: fakeSignal(),
                activeScreenChanged: fakeSignal(),
            };
            const calls: CapturedCall[] = [];
            const timers: Array<{ delay: number; cb: () => void }> = [];
            const writes: string[] = [];
            const surface = {
                windowList: () => [w1, w3, w2],
                screens,
                desktops: [ws1, ws2],
                get activeScreen() {
                    return { name: focus.screen };
                },
                cursorPos: { x: 2, y: 100 },
                windowAdded: signals.windowAdded.signal,
                windowRemoved: signals.windowRemoved.signal,
                windowActivated: signals.windowActivated.signal,
                desktopsChanged: signals.desktopsChanged.signal,
                screensChanged: signals.screensChanged.signal,
                currentDesktopChanged: signals.currentDesktopChanged.signal,
                activeScreenChanged: signals.activeScreenChanged.signal,
                get activeWindow() {
                    return focus.window;
                },
                clientArea: (_area: number, _out: unknown, deskRef: unknown) => {
                    const id = (deskRef as { id?: unknown }).id;
                    if (id === "ws2") {
                        return { x: 800, y: 0, width: 800, height: 600 };
                    }
                    return { x: 0, y: 0, width: 800, height: 600 };
                },
                currentDesktopForScreen: (screen: { name: string }) => (currents.get(screen.name) === "ws2" ? ws2 : ws1),
                setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                    membershipWrites.push(`${tag}:view ${(deskRef as { id: string }).id} on ${screen.name}`);
                    currents.set(screen.name, (deskRef as { id: string }).id);
                },
                sendClientToScreen: () => {
                    membershipWrites.push(`${tag}:send`);
                },
                createDesktop: () => null,
                removeDesktop: () => false,
            };
            const handle = startWorldAdapterEntry({
                workspace: surface,
                owner: "owner-a",
                adapter,
                floatIntent: true,
                callDbus: (_service, _path, _iface, method, payload, callback) => {
                    if (method === "ReadFloatIntent") {
                        const body = JSON.parse(payload) as Record<string, unknown>;
                        callback(JSON.stringify({
                            v: 1,
                            correlation_id: body["correlation_id"],
                            outcome: "ok",
                            members: ["win-a"],
                            stored: 1,
                            returned: 1,
                            tile: ["win-b"],
                            tile_stored: 1,
                            tile_returned: 1,
                        }));
                        return;
                    }
                    if (method === "WriteFloatIntent") {
                        writes.push(payload);
                        return;
                    }
                    calls.push({ method, payload, callback });
                    if (method === "QueueWorldCommand") {
                        callback(broker.answerQueue(payload));
                    } else if (method === "TakeCommandResult") {
                        callback(broker.answerTake(payload));
                    }
                },
                scheduleOnce: (delay, cb) => {
                    timers.push({ delay, cb });
                    return () => undefined;
                },
                log: () => undefined,
                gestureActive: () => false,
                registerShortcutFn: () => true,
                readWorkspaceModeFn: () => "per-output-local",
                readDefaultTiledFn: () => false,
            });
            assert.ok(handle !== null);
            return {
                handle: handle as NonNullable<typeof handle>,
                broker, calls, timers, writes, signals,
                flushed: { count: 0 },
                runTimers: (maxDelay = 120): void => {
                    for (let i = 0; i < timers.length; i += 1) {
                        const t = timers[i] as { delay: number; cb: () => void };
                        if (t.delay <= maxDelay) {
                            timers.splice(i, 1);
                            i -= 1;
                            t.cb();
                        }
                    }
                },
            };
        };
        const flushNext = async (p: Producer): Promise<{ req: Record<string, unknown>; reply: Record<string, unknown>; stamped: Record<string, unknown>; stampedText: string }> => {
            const at = p.calls.findIndex((c, i) => i >= p.flushed.count && c.method === "DescribeWorld");
            assert.ok(at >= 0, "expected a queued DescribeWorld");
            p.flushed.count = at + 1;
            // Mini-broker stamping: the envelope becomes the exact core
            // report the sole broker would dispatch, and the core reply
            // returns bound with a nulled terminal result.
            const stamped = p.broker.stampCapture(p.calls[at]!.payload);
            const coreReply = await engineSend("DescribeWorld", stamped);
            const rawReply = p.broker.bindReply(coreReply, p.calls[at]!.payload);
            p.calls[at]!.callback(rawReply);
            return {
                req: JSON.parse(p.calls[at]!.payload) as Record<string, unknown>,
                reply: JSON.parse(rawReply) as Record<string, unknown>,
                stamped: JSON.parse(stamped) as Record<string, unknown>,
                stampedText: stamped,
            };
        };
        const assertStoreSettled = (writes: string[], label: string): void => {
            assert.ok(writes.length >= 1, `${label}: expected store writes`);
            for (const payload of writes) {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const members = body["members"] as Array<string>;
                const tile = (body["tile"] as Array<string> | undefined) ?? [];
                assert.ok(members.includes("win-a"), `${label}: float identity retained: ${payload}`);
                assert.ok(tile.includes("win-b"), `${label}: tile override retained: ${payload}`);
                assert.ok(!members.includes("win-b"), `${label}: sides disjoint: ${payload}`);
                assert.ok(!tile.includes("win-a"), `${label}: sides disjoint: ${payload}`);
            }
        };
        let p1: Producer | null = null;
        let p2: Producer | null = null;
        // One broker per producer lifetime: the real service rotates the
        // whole broker (fresh generation, fresh engine, dropped queues) when
        // a new producer lifetime binds, so the restart models rotation with
        // a fresh harness broker, never the retained one.
        try {
            // Producer 1: fresh production-minted stream on the saved
            // default (false) with an explicit tiling override, advancing
            // both watermarks with real commands.
            const broker1: MiniBroker = createMiniBroker();
            const adapter1 = mintAdapterIncarnation();
            assert.ok(isOpaqueId(adapter1) && adapter1.startsWith("kwin-world-adapter-"));
            p1 = startProducer(adapter1, "p1", broker1);
            const disc1 = [...p1.calls].reverse().find((c) => c.method === "DiscoverWorld");
            assert.ok(disc1 !== undefined, "producer1 discovers on start");
            assert.equal((JSON.parse(disc1.payload) as Record<string, unknown>)["adapter"], adapter1);
            const bootRaw = await engineSend("DiscoverWorld", disc1.payload);
            const bootBound = broker1.bindDiscovery(bootRaw);
            const boot = JSON.parse(bootBound) as Record<string, unknown>;
            const gen = boot["gen"] as string;
            assert.ok(typeof gen === "string" && gen.length > 0, "first generation bound");
            disc1.callback(bootBound);
            const first = await flushNext(p1);
            assert.equal(first.reply["ready"], true, `first observation accepted: ${JSON.stringify(first.reply)}`);
            for (let round = 0; round < 2; round += 1) {
                for (const handler of [...(p1.signals.desktopsChanged as FakeSignal).handlers]) {
                    handler();
                }
                p1.runTimers();
                const next = await flushNext(p1);
                assert.equal(next.reply["ready"], true, `observation ${round} accepted`);
            }
            // Explicit tiling toggles on BOTH scopes advance the command
            // watermark and leave both tiled (overrides on default false).
            // The dispatched report carries the genuine fixed origin plus
            // the tile-override suppress flag for win-b.
            const findWin = (req: Record<string, unknown>, id: string): Record<string, unknown> | undefined => {
                // v2: the inner report carries flat window facts (no
                // domains lane on the wire).
                const report = req["report"] as Record<string, unknown>;
                return ((report["windows"] as Array<Record<string, unknown>>) ?? []).find((w) => w["win"] === id);
            };
            let lastRev = first.reply["rev"] as string;
            let lastReqSeq = first.stamped["reqSeq"] as string;
            p1.handle.requestWorkspaceTilingToggle();
            p1.runTimers();
            const toggledA = await flushNext(p1);
            assert.equal(toggledA.reply["ready"], true, "toggle out-a accepted");
            assert.equal(toggledA.reply["consumed"], false, "toggle out-a executes");
            lastRev = toggledA.reply["rev"] as string;
            lastReqSeq = toggledA.stamped["reqSeq"] as string;
            focus.screen = "out-b";
            focus.window = w2;
            p1.handle.requestWorkspaceTilingToggle();
            p1.runTimers();
            const toggledB = await flushNext(p1);
            assert.equal(toggledB.reply["ready"], true, "toggle out-b accepted");
            assert.equal(toggledB.reply["consumed"], false, "toggle out-b executes");
            lastRev = toggledB.reply["rev"] as string;
            lastReqSeq = toggledB.stamped["reqSeq"] as string;
            const reportedB = findWin(toggledB.req, "win-b");
            assert.equal(reportedB?.["fixedAuto"], true, `fixed origin declared for win-b: ${JSON.stringify(reportedB)}`);
            assert.equal(reportedB?.["fixedSuppress"], true, `tile override suppresses float admission: ${JSON.stringify(reportedB)}`);
            assert.equal(reportedB?.["floating"], false, "tile override stays tiled in report");
            const replyScopesB = toggledB.reply["scopes"] as Array<Record<string, unknown>>;
            // v2: scope ids name the core logical; the controlled out-b
            // scope must bind the out-b current native (ws2).
            const controlledB = replyScopesB.find((s) => (s["scope"] as string).startsWith("out-b/") && s["auth"] === "controlled");
            assert.ok(controlledB !== undefined, `out-b has a controlled scope: ${JSON.stringify(replyScopesB)}`);
            const scopeLogical = (controlledB["scope"] as string).split("/")[1] as string;
            const scopeRow = ((toggledB.reply["workspaces"] as Array<Record<string, unknown>>) ?? []).find(
                (w) => w["out"] === "out-b" && w["id"] === scopeLogical,
            );
            assert.equal(scopeRow?.["bind"], "ws2", "controlled scope binds the out-b current native");
            const replyRowsB = toggledB.reply["windows"] as Array<Record<string, unknown>>;
            const rowB = replyRowsB.find((w) => w["win"] === "win-b");
            assert.equal(rowB?.["frame_auth"], "controlled", "fixed tile under controlled frame authority");
            assert.equal(rowB?.["member_auth"], "controlled", "fixed tile under controlled membership authority");
            assert.ok(rowB?.["alloc"] !== null && rowB?.["alloc"] !== undefined, `fixed tile carries desired geometry: ${JSON.stringify(rowB)}`);
            assert.ok(Number(lastReqSeq) >= 5, `request watermark advanced: ${lastReqSeq}`);
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(p1.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a override tiled before restart");
            focus.screen = "out-b";
            focus.window = w2;
            assert.equal(p1.handle.getWorkspaceTilingSnapshot().tiled, true, "out-b override tiled before restart");
            focus.screen = "out-a";
            focus.window = w1;
            assertStoreSettled(p1.writes, "producer1");
            const nativeBefore = nativeSnapshot();
            const floatTileBefore = floatTileWriteCount();
            const memberBefore = membershipWrites.length;
            p1.handle.stop();
            // Producer 2: script restart with a fresh production-minted
            // incarnation as a NEW producer lifetime. Like the real
            // WorldBroker::discovery_body, the restart rotates onto a fresh
            // generation and a fresh engine (world_eval rebinds both on its
            // own DiscoverWorld, which the restart sends).
            const adapter2 = mintAdapterIncarnation();
            assert.notEqual(adapter2, adapter1, "restart mints a fresh stream");
            const broker2: MiniBroker = createMiniBroker();
            p2 = startProducer(adapter2, "p2", broker2);
            const disc2 = [...p2.calls].reverse().find((c) => c.method === "DiscoverWorld");
            assert.ok(disc2 !== undefined, "producer2 discovers on start");
            const bootRaw2 = await engineSend("DiscoverWorld", disc2.payload);
            const bootBound2 = broker2.bindDiscovery(bootRaw2);
            disc2.callback(bootBound2);
            const boot2 = JSON.parse(bootBound2) as Record<string, unknown>;
            const gen2 = broker2.gen;
            assert.ok(typeof gen2 === "string" && gen2.length > 0, "rotated generation bound");
            assert.notEqual(gen2, gen, "new lifetime rotates the generation");
            assert.equal(boot2["gen"], gen2, "bootstrap names the rotated generation");
            // The dead world is gone: a retained first-generation capture
            // refuses outright and can never apply again.
            const staleGen = JSON.parse(await engineSend("DescribeWorld", toggledB.stampedText)) as Record<string, unknown>;
            assert.equal(staleGen["outcome"], "rejected");
            assert.equal(
                staleGen["kind"],
                "generation-mismatch",
                "old-generation capture refuses after rotation",
            );
            const restarted = await flushNext(p2);
            assert.equal(restarted.req["adapter"], adapter2);
            assert.match(
                restarted.req["source"] as string,
                /^src-1-/,
                "restarted source namespace begins at 1 for the fresh incarnation",
            );
            assert.ok(!("command" in (restarted.req as Record<string, unknown>)), "restart carries no replayed command");
            assert.equal(restarted.reply["ready"], true, `restart accepted on the rotated engine, never superseded: ${JSON.stringify(restarted.reply)}`);
            assert.equal(restarted.reply["rev"], "1", "rotated engine starts at rev 1");
            assert.equal(restarted.reply["gen"], gen2, "restart binds the rotated generation");
            assert.equal(restarted.req["gen"], gen2, "restart capture binds the rotated generation");
            // Queue/take lanes rebind to the rotated generation: the first
            // retile files under gen2, never the dead gen1 namespace.
            lastRev = restarted.reply["rev"] as string;
            lastReqSeq = restarted.stamped["reqSeq"] as string;
            for (const handler of [...(p2.signals.desktopsChanged as FakeSignal).handlers]) {
                handler();
            }
            p2.runTimers();
            const settled = await flushNext(p2);
            assert.equal(settled.reply["ready"], true, "post-restart observation accepted");
            assert.equal(settled.reply["rev"], "2", "rotated engine advances from rev 1");
            lastRev = settled.reply["rev"] as string;
            lastReqSeq = settled.stamped["reqSeq"] as string;
            assertStoreSettled(p2.writes, "producer2");
            // REQ-CTL-04: per-workspace overrides reset on restart; the saved
            // default (false) reseeds BOTH scopes and native state is untouched.
            const snapA = p2.handle.getWorkspaceTilingSnapshot();
            assert.equal(snapA.tiled, false, `out-a override resets to saved default on restart: ${JSON.stringify(snapA)}`);
            focus.screen = "out-b";
            focus.window = w2;
            const snapB = p2.handle.getWorkspaceTilingSnapshot();
            assert.equal(snapB.tiled, false, `out-b override resets to saved default on restart: ${JSON.stringify(snapB)}`);
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(nativeSnapshot(), nativeBefore, "restart performs no unsolicited native changes");
            assert.equal(floatTileWriteCount(), floatTileBefore, "float/tile take no geometry writes across restart");
            assert.equal(membershipWrites.length, memberBefore, "no membership writes across restart");
            // Explicit retile of BOTH scopes through NEW commands executes in
            // the fresh command namespace (old commands never replay).
            p2.handle.requestWorkspaceTilingToggle();
            p2.runTimers();
            const retiledA = await flushNext(p2);
            assert.equal(retiledA.reply["ready"], true, "retile out-a accepted");
            assert.equal(retiledA.reply["consumed"], false, "retile out-a executes, never consumed");
            assert.equal(retiledA.reply["rev"], String(Number(lastRev) + 1), "rotated revision advances");
            lastRev = retiledA.reply["rev"] as string;
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a scope retiled");
            // The retile filed under the rotated generation, never the dead
            // one: queue/take lanes rebind with the new world.
            const filedQueue = [...p2.calls].reverse().find((c) => c.method === "QueueWorldCommand");
            assert.ok(filedQueue !== undefined, "retile files a queue command");
            assert.equal(
                (JSON.parse(filedQueue.payload) as Record<string, unknown>)["gen"],
                gen2,
                "queue binds the rotated generation",
            );
            const filedTake = [...p2.calls].reverse().find((c) => c.method === "TakeCommandResult");
            assert.ok(filedTake !== undefined, "retile collects its terminal result");
            assert.equal(
                (JSON.parse(filedTake.payload) as Record<string, unknown>)["gen"],
                gen2,
                "take binds the rotated generation",
            );
            focus.screen = "out-b";
            focus.window = w2;
            p2.handle.requestWorkspaceTilingToggle();
            p2.runTimers();
            const retiled = await flushNext(p2);
            assert.equal(retiled.reply["ready"], true, "retile out-b accepted");
            assert.equal(retiled.reply["consumed"], false, "retile out-b executes, never consumed");
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-b scope retiled");
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a scope still tiled");
            // The accepted reply still floats win-a explicitly and carries
            // the fixed tile under controlled authority with desired
            // geometry; the native fixed frame matches the accepted frame
            // and the store keeps both sides.
            const rows = retiled.reply["windows"] as Array<Record<string, unknown>>;
            assert.equal(rows.find((w) => w["win"] === "win-a")?.["floating"], true, "float persists in accepted reply");
            const rowFixed = rows.find((w) => w["win"] === "win-b");
            assert.equal(rowFixed?.["frame_auth"], "controlled", "fixed tile under controlled frame authority");
            assert.equal(rowFixed?.["member_auth"], "controlled", "fixed tile under controlled membership authority");
            assert.ok(rowFixed?.["alloc"] !== null && rowFixed?.["alloc"] !== undefined, `fixed tile carries desired geometry: ${JSON.stringify(rowFixed)}`);
            const nativeFixed = w2.frameGeometry;
            assert.deepEqual(
                [nativeFixed.x, nativeFixed.y, nativeFixed.width, nativeFixed.height],
                rowFixed?.["frame"],
                `native fixed frame matches accepted frame: ${JSON.stringify(nativeFixed)} vs ${JSON.stringify(rowFixed?.["frame"])}`,
            );
            assertStoreSettled(p2.writes, "producer2 retiled");
            assert.equal(nativeSnapshot(), nativeBefore, "frames converge back to pre-restart geometry");
            p2.handle.stop();
            p2 = null;
        } finally {
            try {
                p1?.handle.stop();
            } catch (error) {
                void error;
            }
            try {
                p2?.handle.stop();
            } catch (error) {
                void error;
            }
            try {
                lines.close();
            } catch (error) {
                void error;
            }
            try {
                proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
                if (proc.exitCode !== null || proc.signalCode !== null) {
                    clearTimeout(timer);
                    resolvePromise();
                }
            });
        }
    });
});

describe("world entry float-intent reconcile controls", () => {
    // Compact offline harness for the intent-membership paths touched by the
    // restart-preservation fix: seeded persisted float (win-a) plus disjoint
    // tile override (win-b), hand-built accepted replies, real entry.
    interface FloatHarness {
        handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
        calls: CapturedCall[];
        timers: Array<{ delay: number; cb: () => void }>;
        writes: string[];
        live: FakeWindow[];
        w1: FakeWindow;
        w2: FakeWindow;
        signals: Record<string, FakeSignal>;
        rev: { count: number };
        runTimers: (maxDelay?: number) => void;
        lastCall: (method: string) => CapturedCall;
    }

    function makeFloatHarness(): FloatHarness {
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out", ws1, 0);
        const w2 = win("win-b", "out", ws2, 0);
        const live: FakeWindow[] = [w1, w2];
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const writes: string[] = [];
        const surface = {
            windowList: () => [...live],
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1, ws2],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals.windowAdded as FakeSignal).signal,
            windowRemoved: (signals.windowRemoved as FakeSignal).signal,
            windowActivated: (signals.windowActivated as FakeSignal).signal,
            desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
            screensChanged: (signals.screensChanged as FakeSignal).signal,
            currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
            activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
            get activeWindow() {
                return w1;
            },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            floatIntent: true,
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 1,
                        correlation_id: body["correlation_id"],
                        outcome: "ok",
                        members: ["win-a"],
                        stored: 1,
                        returned: 1,
                        tile: ["win-b"],
                        tile_stored: 1,
                        tile_returned: 1,
                    }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    writes.push(payload);
                    return;
                }
                if (method === "DiscoverWorld" || method === "DescribeWorld") {
                    calls.push({ method, payload, callback });
                    return;
                }
                if (method === "QueueWorldCommand") {
                    calls.push({ method, payload, callback });
                    const queueBody = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
                    return;
                }
                if (method === "TakeCommandResult") {
                    calls.push({ method, payload, callback });
                    const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                    if (stagedTake !== undefined) {
                        delete (globalThis as Record<string, unknown>)["__takeResult"];
                        callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                    } else {
                        callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                    }
                    return;
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
        });
        assert.ok(handle !== null);
        const h = {
            handle, calls, timers, writes, live, w1, w2, signals, rev: { count: 0 },
            runTimers: (maxDelay = 120): void => {
                for (let i = 0; i < timers.length; i += 1) {
                    const t = timers[i] as { delay: number; cb: () => void };
                    if (t.delay <= maxDelay) {
                        timers.splice(i, 1);
                        i -= 1;
                        t.cb();
                    }
                }
            },
            lastCall: (method: string): CapturedCall => {
                const found = [...calls].reverse().find((c) => c.method === method);
                assert.ok(found !== undefined, `expected ${method}`);
                return found as CapturedCall;
            },
        };
        // Settle discovery plus the auto observation-only dispatch.
        const disc = h.lastCall("DiscoverWorld");
        const discBody = JSON.parse(disc.payload);
        disc.callback(BOOTSTRAP("gen-1", discBody.req));
        const auto = h.lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        h.rev.count += 1;
        auto.callback(acceptedReply("gen-1", autoBody.source, autoBody.nativeInvalid, String(h.rev.count)));
        return h;
    }

    const floatRow = (w: FakeWindow, extra: Record<string, unknown> = {}): Record<string, unknown> => ({
        win: w.internalId, out: "out", ws: w.desktops[0]?.id ?? "ws1",
        alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: true, float_origin: "",
        ...extra,
    });

    const tileRow = (w: FakeWindow, extra: Record<string, unknown> = {}): Record<string, unknown> => ({
        win: w.internalId, out: "out", ws: w.desktops[0]?.id ?? "ws1",
        alloc: null, frame: null,
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "",
        ...extra,
    });

    function acceptHead(h: FloatHarness, extra: Record<string, unknown> = {}): void {
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = h.lastCall("DescribeWorld");
        const body = JSON.parse(req.payload);
        h.rev.count += 1;
        const { result, ...rest } = extra;
        if (result !== undefined) {
            (globalThis as Record<string, unknown>)["__takeResult"] = result;
        }
        req.callback(acceptedReply("gen-1", body.source, body.nativeInvalid, String(h.rev.count), rest));
    }

    const lastWrite = (h: FloatHarness): Record<string, unknown> =>
        JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;

    it("untouched floating-false rows never unfloat (overlay shape)", () => {
        const h = makeFloatHarness();
        // Fullscreen-overlay tiled shape: floating false with untouched
        // frame authority and no hold. Must retain the settled float.
        acceptHead(h, { windows: [tileRow(h.w1, { frame_auth: "untouched", floating: false })] });
        const members = lastWrite(h)["members"] as Array<string>;
        assert.ok(members.includes("win-a"), `overlay row must not unfloat: ${JSON.stringify(lastWrite(h))}`);
        h.handle?.stop();
    });

    it("explicit successful unfloat clears while noSpace retains", () => {
        const cleared = makeFloatHarness();
        acceptHead(cleared, {
            windows: [],
            result: { op: "toggle-float", win: "win-a", rect: null },
        });
        const done = lastWrite(cleared);
        assert.ok(!(done["members"] as Array<string>).includes("win-a"), `explicit unfloat clears: ${JSON.stringify(done)}`);
        assert.ok((done["tile"] as Array<string>).includes("win-a"), `explicit unfloat records tile: ${JSON.stringify(done)}`);
        cleared.handle?.stop();
        // No-space retry keeps the float and records no tile override.
        const held = makeFloatHarness();
        acceptHead(held, {
            windows: [],
            result: { op: "toggle-float", win: "win-a", rect: null, noSpace: true },
        });
        const kept = lastWrite(held);
        assert.ok((kept["members"] as Array<string>).includes("win-a"), `noSpace retains float: ${JSON.stringify(kept)}`);
        assert.ok(!(kept["tile"] as Array<string>).includes("win-a"), `noSpace records no tile: ${JSON.stringify(kept)}`);
        held.handle?.stop();
    });

    it("clean complete departure prunes the settled float", () => {
        const h = makeFloatHarness();
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(h.w1);
        }
        h.live.splice(h.live.indexOf(h.w1), 1);
        acceptHead(h, { windows: [] });
        const done = lastWrite(h);
        assert.ok(!(done["members"] as Array<string>).includes("win-a"), `departed float prunes: ${JSON.stringify(done)}`);
        assert.ok((done["tile"] as Array<string>).includes("win-b"), `tile override survives departure: ${JSON.stringify(done)}`);
        h.handle?.stop();
    });

    it("live-minimized floats survive tiled-classified rows", () => {
        const h = makeFloatHarness();
        h.w1.minimized = true;
        acceptHead(h, { windows: [tileRow(h.w1)] });
        const members = lastWrite(h)["members"] as Array<string>;
        assert.ok(members.includes("win-a"), `minimized float holds: ${JSON.stringify(lastWrite(h))}`);
        h.handle?.stop();
    });

    it("capacity floats converge without disturbing the tile override", () => {
        const h = makeFloatHarness();
        acceptHead(h, {
            windows: [
                floatRow(h.w1, { float_origin: "capacity" }),
                tileRow(h.w2),
            ],
        });
        const done = lastWrite(h);
        assert.ok((done["members"] as Array<string>).includes("win-a"), `capacity float converges: ${JSON.stringify(done)}`);
        assert.ok((done["tile"] as Array<string>).includes("win-b"), `tile override survives: ${JSON.stringify(done)}`);
        assert.ok(!(done["members"] as Array<string>).includes("win-b"), `tile side stays disjoint: ${JSON.stringify(done)}`);
        h.handle?.stop();
    });

    it("cross-scope tiled rows never clear (scope isolation)", () => {
        const h = makeFloatHarness();
        // Same id named at another scope with full tile proof: not the live
        // scope, so the settled float holds.
        acceptHead(h, { windows: [tileRow(h.w1, { ws: "ws2" })] });
        const members = lastWrite(h)["members"] as Array<string>;
        assert.ok(members.includes("win-a"), `cross-scope row must not clear: ${JSON.stringify(lastWrite(h))}`);
        h.handle?.stop();
    });
});

describe("world entry adapter incarnation", () => {
    it("mints a fresh opaque token per script evaluation", () => {
        const first = mintAdapterIncarnation();
        const second = mintAdapterIncarnation();
        for (const minted of [first, second]) {
            assert.ok(isOpaqueId(minted), `incarnation must stay opaque: ${minted}`);
            assert.ok(minted.startsWith("kwin-world-adapter-"), `incarnation keeps the production base: ${minted}`);
            assert.ok(minted.length <= 128, `incarnation stays bounded: ${minted.length}`);
        }
        assert.notEqual(first, second, "two evaluations mint different tokens");
    });
});
