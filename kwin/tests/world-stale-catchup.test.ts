// Stale-reply liveness through the production world entry (mocked transport,
// mock-time queue): a consumed flight rejected for freshness/correlation
// must release exactly its flight token so one observation-only catchup
// redispatches (no wedge, no command replay), and later commands apply.
// State-only tests cannot observe this: the wedge is the entry holding the
// consumed flight record while its callback already settled.

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
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: null;
    maxSize: null;
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}
interface FakeSignal {
    readonly handlers: Array<() => void>;
    readonly signal: { connect: (handler: () => void) => void; disconnect: (handler: () => void) => void };
}

const desk = (id: string): FakeDesktop => ({ id });

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

interface Harness {
    world: {
        wins: FakeWindow[];
        activeWindow: FakeWindow | null;
        currentByScreen: Map<string, FakeDesktop>;
        focused: string[];
        geometryWrites: string[];
        membershipWrites: string[];
        viewSwitches: string[];
        created: string[];
        removed: string[];
    };
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    signals: Record<string, FakeSignal>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runDueTimers: () => void;
}

function makeHarness(): Harness {
    const ws1 = desk("ws1");
    const wins: FakeWindow[] = [
        {
            internalId: "win-a",
            normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" },
            desktops: [ws1],
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: null,
            maxSize: null,
        },
        {
            internalId: "win-b",
            normalWindow: true,
            frameGeometry: { x: 220, y: 0, width: 200, height: 200 },
            output: { name: "out" },
            desktops: [ws1],
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: null,
            maxSize: null,
        },
    ];
    const world = {
        wins,
        activeWindow: wins[0] ?? null,
        currentByScreen: new Map([["out", ws1]]),
        focused: [] as string[],
        geometryWrites: [] as string[],
        membershipWrites: [] as string[],
        viewSwitches: [] as string[],
        created: [] as string[],
        removed: [] as string[],
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const signals: Record<string, FakeSignal> = { windowActivated: fakeSignal() };
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
    const desktops = [ws1];
    const surface = {
        windowList: () => world.wins,
        screens,
        desktops,
        activeScreen: { name: "out" },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.focused.push(found.internalId);
            }
        },
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            const found = desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentByScreen.set(screen.name, found);
                world.viewSwitches.push(`${screen.name}->${found.id}`);
            }
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => {
            world.created.push("native");
            return null as unknown as object;
        },
        removeDesktop: () => {
            world.removed.push("native");
        },
    };
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
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    // Bounded mock-time: fire only debounce timers (<=120ms), never the
    // 2000ms transport timeouts. Snapshot per pass with a cap so an
    // infinite re-arm would fail loudly instead of hanging.
    const runDueTimers = (): void => {
        for (let pass = 0; pass < 10; pass += 1) {
            const due = timers.filter((t) => t.delay <= 120);
            if (due.length === 0) {
                return;
            }
            for (const t of due) {
                const index = timers.indexOf(t);
                if (index >= 0) {
                    timers.splice(index, 1);
                }
            }
            for (const t of due) {
                t.cb();
            }
        }
        throw new Error("timer queue did not settle: infinite re-arm");
    };
    return { world, calls, timers, logs, signals, handle, runDueTimers };
}

function lastDescribe(h: Harness): { payload: Record<string, unknown>; callback: (reply: unknown) => void } {
    const found = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(found !== undefined, "expected a DescribeWorld dispatch");
    return { payload: JSON.parse(found.payload) as Record<string, unknown>, callback: found.callback };
}

function describeCount(h: Harness): number {
    return h.calls.filter((c) => c.method === "DescribeWorld").length;
}

function discover(h: Harness, gen = "gen-1"): void {
    const disc = h.calls.find((c) => c.method === "DiscoverWorld");
    assert.ok(disc !== undefined);
    const req = (JSON.parse(disc.payload) as Record<string, unknown>)["req"];
    disc.callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req,
        obs: "0", rev: "0", ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [],
    }));
    const auto = lastDescribe(h);
    assert.equal(auto.payload["command"], null);
    auto.callback(acceptedReply(gen, auto.payload, "1"));
}

function acceptedReply(gen: string, dispatched: Record<string, unknown>, rev: string): string {
    return JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req: dispatched["req"], obs: dispatched["obs"], rev,
        ready: true, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [],
    });
}

// A stale reply carrying desired rows that WOULD actuate if applied: a
// differing controlled frame, a view transfer, and a focus effect.
function staleReply(gen: string, dispatched: Record<string, unknown>, rev: string, overrides: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req: dispatched["req"], obs: dispatched["obs"], rev,
        ready: true, consumed: false,
        scopes: [],
        workspaces: [],
        outputs: [],
        windows: [{
            win: "win-a", out: "out", ws: "ws1",
            alloc: [9, 9, 300, 300], frame: [9, 9, 300, 300],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "",
            floating: false, float_origin: "",
        }],
        transfers: [{ op: "op-9", kind: "view", win: "", out: "out", ws: "ws1", follow: false, held: false }],
        effects: [{ id: "fx-9", kind: "focus", target: "win-a", rev }],
        ...overrides,
    });
}

function settersFired(h: Harness): string[] {
    return [
        ...h.world.geometryWrites.map((w) => `geometry:${w}`),
        ...h.world.membershipWrites.map((w) => `membership:${w}`),
        ...h.world.viewSwitches.map((w) => `view:${w}`),
        ...h.world.focused.map((w) => `focus:${w}`),
        ...h.world.created.map((w) => `create:${w}`),
        ...h.world.removed.map((w) => `remove:${w}`),
    ];
}

function fireNativeEvent(h: Harness): void {
    for (const handler of [...(h.signals["windowActivated"] as FakeSignal).handlers]) {
        handler();
    }
}

describe("world stale-reply catchup liveness", () => {
    it("stale-obs drops setters, redispatches once observation-only, then acks and accepts later commands", () => {
        const h = makeHarness();
        discover(h);
        // Explicit command flight: the stale drop must not replay it.
        h.handle?.requestWorkspaceRelative(1);
        h.runDueTimers();
        const flight = lastDescribe(h);
        assert.deepEqual((flight.payload["command"] as Record<string, unknown>)["op"], "select-relative");
        // Native event advances the watermark past the flight obs.
        fireNativeEvent(h);
        // Stale reply: exact req/adapter/gen, newer rev, old obs, with
        // desired rows that would actuate if applied.
        flight.callback(staleReply("gen-1", flight.payload, "9"));
        assert.ok(h.logs.some((l) => l.includes("stale reason=stale-obs") && l.includes("recovery=catchup")));
        assert.deepEqual(settersFired(h), [], "stale reply performs zero native setters");
        // Exactly one observation-only catchup redispatch, no replay.
        const before = describeCount(h);
        h.runDueTimers();
        assert.equal(describeCount(h), before + 1, "one catchup dispatch, no wedge");
        const catchup = lastDescribe(h);
        assert.equal(catchup.payload["command"], null, "catchup is observation-only: the dropped command never replays");
        // Accepted ack unwedges: a newer rev on the fresh flight applies.
        catchup.callback(acceptedReply("gen-1", catchup.payload, "2"));
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:applied rev=2")));
        // A later explicit command dispatches and applies normally.
        h.handle?.requestWorkspaceRelative(1);
        h.runDueTimers();
        const later = lastDescribe(h);
        assert.deepEqual((later.payload["command"] as Record<string, unknown>)["op"], "select-relative");
        later.callback(acceptedReply("gen-1", later.payload, "3"));
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:applied rev=3")));
    });

    it("wrong-req replies apply nothing and ack nothing but still redispatch observation-only", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runDueTimers();
        const flight = lastDescribe(h);
        // Delayed arbitrary reply on the consumed callback: same gen/obs,
        // high rev, but the wrong req. Must not ack rev 9.
        flight.callback(staleReply("gen-1", flight.payload, "9", { req: "req-999" }));
        assert.ok(h.logs.some((l) => l.includes("stale reason=stale-req")));
        assert.deepEqual(settersFired(h), [], "wrong-req reply performs zero native setters");
        const before = describeCount(h);
        h.runDueTimers();
        assert.equal(describeCount(h), before + 1, "wrong-req consumes its flight yet redispatches: no wedge");
        const catchup = lastDescribe(h);
        assert.equal(catchup.payload["command"], null);
        // The high rev was never acked: a lower rev still applies.
        catchup.callback(acceptedReply("gen-1", catchup.payload, "2"));
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:applied rev=2")), "unacked wrong rev does not poison ordering");
    });

    it("wrong-adapter replies apply nothing and ack nothing but still redispatch observation-only", () => {
        const h = makeHarness();
        discover(h);
        fireNativeEvent(h);
        h.runDueTimers();
        const flight = lastDescribe(h);
        flight.callback(staleReply("gen-1", flight.payload, "9", { adapter: "other-adapter" }));
        assert.ok(h.logs.some((l) => l.includes("stale reason=adapter-mismatch")));
        assert.deepEqual(settersFired(h), [], "foreign-adapter reply performs zero native setters");
        const before = describeCount(h);
        h.runDueTimers();
        assert.equal(describeCount(h), before + 1, "foreign reply consumes its flight yet redispatches: no wedge");
        assert.equal((lastDescribe(h).payload["command"] as null), null);
        const catchup = lastDescribe(h);
        catchup.callback(acceptedReply("gen-1", catchup.payload, "2"));
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:applied rev=2")));
    });

    it("duplicate-rev replies release the flight without a spurious re-report", () => {
        const h = makeHarness();
        discover(h);
        fireNativeEvent(h);
        h.runDueTimers();
        const flight = lastDescribe(h);
        // Same req/obs/adapter, but rev equal to the last accepted: stale.
        // The observation already reached the core, so no re-report is
        // owed — but the consumed flight must release.
        flight.callback(staleReply("gen-1", flight.payload, "1"));
        assert.ok(h.logs.some((l) => l.includes("stale reason=stale-rev")));
        assert.deepEqual(settersFired(h), [], "duplicate-rev reply performs zero native setters");
        const clean = describeCount(h);
        h.runDueTimers();
        assert.equal(describeCount(h), clean, "matched-obs duplicate schedules no spurious re-report");
        // The next native event dispatches normally: no wedge.
        fireNativeEvent(h);
        h.runDueTimers();
        assert.equal(describeCount(h), clean + 1, "stale-rev releases the consumed flight");
        assert.equal(lastDescribe(h).payload["command"], null);
    });

    it("pins the Meta+Y toggle to a set-workspace-tiling submission", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceTilingToggle();
        h.runDueTimers();
        const sent = lastDescribe(h);
        assert.deepEqual(sent.payload["command"], {
            op: "set-workspace-tiling", out: "out", ws: "ws1", tiled: false,
        });
    });
});
