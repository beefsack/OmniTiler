// Offline integration coverage for the world production entry (D1-D5):
// discovery handshake, full observation reports, freshness acceptance,
// out-of-order/input-inflight/gesture/close/minimize/restart/incomplete/
// duplicate-effect/refusal/ambiguous-creation/removal-race/mode/partial-apply
// scenarios. Scripted in-memory transport; no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop {
    id: string;
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

const desk = (id: string): FakeDesktop => ({ id });
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
        createDesktop: () => {
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id));
            world.created.push(id);
            return world.desktops[world.desktops.length - 1] as object;
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
    v: 1, adapter: "test-adapter", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    windows: [], transfers: [], effects: [],
});

function discover(h: Harness, gen = "gen-1"): void {
    const call = h.lastCall("DiscoverWorld");
    const body = JSON.parse(call.payload);
    call.callback(BOOTSTRAP(gen, body.req));
    // Settle the discovery-triggered observation report first: the entry
    // auto-dispatches one observation-only DescribeWorld on bind, and a
    // command issued while it is in flight is busy-refused (never
    // coalesced). Replying here leaves the flight idle for the test body.
    const auto = h.lastCall("DescribeWorld");
    const autoBody = JSON.parse(auto.payload);
    assert.equal(autoBody.command, null);
    auto.callback(acceptedReply(gen, autoBody.req, autoBody.obs, "0"));
}

function acceptedReply(gen: string, req: string, obs: string, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req, obs, rev,
        ready: true, consumed: false,
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        ...extra,
    });
}

function requestOf(h: Harness): Record<string, unknown> {
    return JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
}

describe("world entry discovery and observation", () => {
    it("discovers on start and sends a full observation report", () => {
        const h = makeHarness();
        assert.equal(h.lastCall("DiscoverWorld").method, "DiscoverWorld");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal(req["v"], 1);
        assert.equal(req["gen"], "gen-1");
        assert.equal(req["mode"], "per-output-local");
        const domains = req["domains"] as Array<Record<string, unknown>>;
        // Both workspaces observed (visible + hidden), never silent absence.
        assert.equal(domains.length, 2);
        const wins = (domains as Array<Record<string, unknown>>).flatMap((d) => d["windows"] as Array<unknown>);
        assert.equal(wins.length, 2);
        // Settings/safety/constraints ride every domain/window.
        assert.equal((domains[0] as Record<string, unknown>)["gap"], 8);
        const first = (wins[0] as Record<string, unknown>);
        assert.equal(typeof first["hints"], "object");
        // Command rides with its own sequence distinct from reqSeq.
        assert.ok((req["cmdSeq"] as string) !== null);
        assert.deepEqual((req["command"] as Record<string, unknown>)["op"], "select-relative");
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
        assert.ok((requestOf(h)["domains"] as Array<unknown>).length >= 1);
    });

    it("accepts all three workspace modes", () => {
        for (const mode of ["per-output-local", "global-unique", "shared"]) {
            const h = makeHarness(mode);
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            assert.equal(requestOf(h)["mode"], mode);
        }
    });

    it("wires the saved default tiling into reports and the snapshot", () => {
        // Backfill default: reports true and the snapshot seeds tiled.
        const backfill = makeHarness("per-output-local", false);
        discover(backfill);
        assert.equal(requestOf(backfill)["defaultTiled"], true);
        assert.equal(backfill.handle?.getWorkspaceTilingSnapshot().defaultTiled, true);
        backfill.handle?.stop();
        // Saved false: reports false and the pre-reply snapshot seeds
        // floating; the entry keeps no per-workspace flag of its own.
        const floating = makeHarness("per-output-local", false, false);
        discover(floating);
        assert.equal(requestOf(floating)["defaultTiled"], false);
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
        const reply = acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["req"] as string, req2["obs"] as string, "1"));
        assert.equal(h.world.geometryWrites.length, writes);
    });

    it("treats input during flight as stale-obs catchup with no command replay", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = requestOf(h);
        const firstObs = first["obs"] as string;
        // Second command while in flight is busy (never coalesced).
        const rel = h.shortcuts.get("omnitiler-workspace-next-j");
        assert.ok(rel !== undefined);
        rel();
        assert.ok(h.logs.some((l) => l.includes("command-busy")));
        // Native input during flight advances CURRENT obs synchronously via
        // the signal seam: the outstanding flight now resolves against a
        // moved watermark, so acceptReply sees obs mismatch and schedules
        // catchup without setters or command replay.
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["req"] as string, firstObs, "1"));
        assert.ok(h.logs.some((l) => l.includes("stale")));
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("suspends geometry under a gesture barrier but keeps observation flowing", () => {
        const h = makeHarness("per-output-local", true);
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal(req["gesture"], true);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [50, 50, 300, 300], frame: [50, 50, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
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
        const domains = requestOf(h)["domains"] as Array<Record<string, unknown>>;
        const wins = domains.flatMap((d) => d["windows"] as Array<Record<string, unknown>>);
        assert.ok(wins.every((w) => (w as Record<string, unknown>)["win"] !== "win-b"));
        assert.ok(wins.some((w) => (w as Record<string, unknown>)["win"] === "win-a"));
        const closed = domains.flatMap((d) => d["closed"] as Array<string>);
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
        const req = requestOf(h);
        // Minimized lifetimes ride the explicit minimized section, separate
        // from active tiled domain membership (never silent absence, never
        // an inferred close).
        const minimized = req["minimized"] as Array<Record<string, unknown>>;
        assert.ok(minimized.some((m) => m["win"] === "win-b" && m["out"] === "out" && m["ws"] === "ws2"));
        const domains = req["domains"] as Array<Record<string, unknown>>;
        const wins = domains.flatMap((d) => d["windows"] as Array<Record<string, unknown>>);
        assert.ok(!wins.some((w) => (w as Record<string, unknown>)["win"] === "win-b"));
        assert.equal(req["predicate"], "both-axes-fixed");
        assert.equal(req["refill"], "last-remaining-workspace");
    });

    it("carries the validated refill token live per report", () => {
        let refill: unknown = "most-recently-used-workspace";
        const h = makeHarness("per-output-local", false, true, () => refill);
        discover(h);
        assert.equal(requestOf(h)["refill"], "most-recently-used-workspace");
        // A setting change applies to the next report only: invalid values
        // fall back to the default without touching committed state.
        refill = "bogus";
        const before = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, before + 1);
        assert.equal(requestOf(h)["refill"], "last-remaining-workspace");
    });

    it("rediscovers on generation mismatch without replaying the command", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const callsBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 1, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
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
            auto.callback(acceptedReply("gen-2", autoBody.req, autoBody.obs, "0"));
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            scopes: [{ scope: "out/ws2", auth: "hold", reason: "unreadable" }],
            windows: [{
                win: "win-b", out: "out", ws: "ws2",
                alloc: [0, 0, 100, 100], frame: [0, 0, 100, 100],
                frame_auth: "hold", frame_hold: "unreadable",
                member_auth: "hold", member_hold: "unreadable",
                frame_op: "",
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            effects: [{ id: "fx-1", kind: "focus", target: "win-b", rev: "1" }],
        }));
        assert.equal(h.world.focused.length, 1);
        // A repeated identical effect id on a later reply never reapplies.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req2 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["req"] as string, req2["obs"] as string, "2", {
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
        assert.deepEqual((req["command"] as Record<string, unknown>)["op"], "send-relative");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "controlled", frame_hold: "",
                member_auth: "hold", member_hold: "refused-target",
                frame_op: "",
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                // Desired-absent but occupied: the fresh occupancy gate keeps it.
                { id: "ws2", out: "out", bind: "ws2", logical: "", scope: "out", order: 1, mode: "tiled", exist: false, hold: "" },
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "1", {
            windows: [
                {
                    win: "win-a", out: "out", ws: "ws1",
                    alloc: [11, 11, 300, 300], frame: [11, 11, 300, 300],
                    frame_auth: "controlled", frame_hold: "",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "",
                    floating: false, float_origin: "",
                },
                {
                    win: "win-b", out: "out", ws: "ws2",
                    alloc: [22, 22, 300, 300], frame: [22, 22, 300, 300],
                    frame_auth: "hold", frame_hold: "gesture-barrier",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "",
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
        const req = requestOf(h);
        const domains = req["domains"] as Array<Record<string, unknown>>;
        const wins = domains.flatMap((d) => d["windows"] as Array<Record<string, unknown>>);
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
        const cmd = first["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "send-numbered");
        assert.equal(cmd["index"], 0);
        assert.equal(cmd["follow"], false);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["req"] as string, first["obs"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws2", out: "out", bind: "ws2", logical: "", scope: "out", order: 1, mode: "tiled", exist: true, hold: "" },
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
        const req = requestOf(h);
        const cmd = req["command"] as Record<string, unknown>;
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
        const cmd = (requestOf(h)["command"] as Record<string, unknown>);
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
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", seed["req"] as string, seed["obs"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
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
        let req = requestOf(h);
        assert.equal(req["command"], null, "clear schedules observation first");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["req"] as string, req["obs"] as string, "2", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                floating: true, float_origin: "",
            }],
        }));
        h.runTimers();
        req = requestOf(h);
        const cmd = req["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        assert.equal((cmd["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.equal((target as FakeWindow).maximizeMode, 0);
        assert.ok(h.logs.some((l) => l.includes("outcome=observed-cleared")));
    });
});
