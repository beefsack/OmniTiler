// B9 deferred maximized-unfloat (D1): the native `setMaximize` settles
// asynchronously on the live host (`maximizeMode` 3->0 arrives after the
// setter returns), so the old same-stack readback always misread
// still-maximized and refused a live settlement. Clear exactly once, then
// use the first fresh accepted full-state report to admit once or end an
// unconfirmed attempt. No setter retries or signal wait. Producer-shaped:
// QML array-likes (never JS arrays), braced `{uuid}` live ids with bare ids
// on the wire, renewed wrappers per enumeration (never ref identity),
// real KWin property names and asynchronous native settlement.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

const WS_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WS_B = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const OUT = "out-1";

const TILE_B: [number, number, number, number] = [400, 0, 400, 600];
const FLOAT_A: [number, number, number, number] = [162, 122, 475, 355];

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

type ClearMode = "sync" | "async-manual" | "stubborn" | "missing";

interface LiveWin {
    frame: { x: number; y: number; width: number; height: number };
    maximizeMode: number;
    fullScreen: boolean;
    ws: string;
}

function makeHarness(mode: ClearMode) {
    const live = new Map<string, LiveWin>();
    live.set(WIN_A, { frame: { x: 0, y: 0, width: 400, height: 600 }, maximizeMode: 0, fullScreen: false, ws: WS_A });
    live.set(WIN_B, { frame: { x: 400, y: 0, width: 400, height: 600 }, maximizeMode: 0, fullScreen: false, ws: WS_A });
    let activeBare: string = WIN_A;
    let gesture = false;
    const geometryWrites: string[] = [];
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    type Handler = (payload?: unknown) => void;
    const sig = () => {
        const handlers: Handler[] = [];
        return {
            handlers,
            signal: {
                connect: (h: Handler): void => {
                    handlers.push(h);
                },
                disconnect: (h: Handler): void => {
                    const i = handlers.indexOf(h);
                    if (i >= 0) {
                        handlers.splice(i, 1);
                    }
                },
            },
        };
    };
    const signals = {
        windowAdded: sig(),
        windowRemoved: sig(),
        windowActivated: sig(),
        desktopsChanged: sig(),
        screensChanged: sig(),
        currentDesktopChanged: sig(),
        activeScreenChanged: sig(),
    };
    const desktops = [{ id: WS_A }];
    const screens = [{ name: OUT, geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];

    const wrap = (bare: string): object => {
        const st = live.get(bare);
        assert.ok(st !== undefined, `live window ${bare}`);
        const w: Record<string, unknown> = {
            internalId: `{${bare}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            output: { name: OUT },
            desktops: qmlLike([{ id: st.ws }]),
            minimized: false,
            hidden: false,
            fullscreen: st.fullScreen,
            maximized: st.maximizeMode !== 0,
            sticky: false,
            keepAbove: false,
            keepBelow: false,
            minSize: { width: 0, height: 0 },
            maxSize: { width: 16777215, height: 16777215 },
            fullScreen: st.fullScreen,
            maximizeMode: st.maximizeMode,
            onAllDesktops: false,
        };
        if (mode !== "missing") {
            w["setMaximize"] = (_v: boolean, _h: boolean): void => {
                const cur = live.get(bare);
                if (cur === undefined) {
                    return;
                }
                if (mode === "sync") {
                    cur.maximizeMode = 0;
                }
                // async-manual/stubborn: no state change here; the test
                // drives settlement (or the lack of it) explicitly.
            };
        }
        let frame = { ...st.frame };
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => ({ ...frame }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = { ...rect };
                const cur = live.get(bare);
                if (cur !== undefined) {
                    cur.frame = { ...rect };
                }
                geometryWrites.push(`${bare}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        return w;
    };

    const surface = {
        windowList: () => qmlLike([...live.keys()].map((bare) => wrap(bare))),
        get screens() {
            return qmlLike(screens.map((s) => ({ ...s })));
        },
        get desktops() {
            return qmlLike(desktops.map((d) => ({ ...d })));
        },
        activeScreen: { name: OUT },
        get activeWindow() {
            return activeBare === "" ? null : { internalId: `{${activeBare}}` };
        },
        currentDesktop: { id: WS_A },
        currentDesktopForScreen: (_screen: unknown) => ({ id: WS_A }),
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        windowAdded: (signals.windowAdded as { signal: unknown }).signal,
        windowRemoved: (signals.windowRemoved as { signal: unknown }).signal,
        windowActivated: (signals.windowActivated as { signal: unknown }).signal,
        desktopsChanged: (signals.desktopsChanged as { signal: unknown }).signal,
        screensChanged: (signals.screensChanged as { signal: unknown }).signal,
        currentDesktopChanged: (signals.currentDesktopChanged as { signal: unknown }).signal,
        activeScreenChanged: (signals.activeScreenChanged as { signal: unknown }).signal,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 3, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 3, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 3, outcome: "pending", gen: "gen-1" }));
                }            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => {
            logs.push(m);
        },
        gestureActive: () => gesture,
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
    const lastCall = (method: string) => {
        const found = [...calls].reverse().find((c) => c.method === method);
        assert.ok(found !== undefined, `expected ${method}`);
        return found as { method: string; payload: string; callback: (reply: unknown) => void };
    };
    const accepted = (
        gen: string,
        source: string,
        nativeInvalid: string,
        rev: string,
        extra: Record<string, unknown> = {},
    ): string => {
        // Terminal results ride the take lane requester-only, never the
        // capture reply.
        const { result, ...snapshot } = extra;
        if (result !== undefined) {
            (globalThis as Record<string, unknown>)["__takeResult"] = result;
        }
        return JSON.stringify({
            v: 3, adapter: "broker-1", gen, req: source, obs: rev, rev,
            ready: true, consumed: false,
            source,
            nativeInvalid,
            clock: "0",
            gestureSettled: null, gestureAssoc: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [{ scope: `${OUT}/${WS_A}`, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
            ...snapshot,
        });
    };
    const discover = (gen = "gen-1"): void => {
        const call = lastCall("DiscoverWorld");
        const body = JSON.parse(call.payload);
        call.callback(JSON.stringify({
            v: 3, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            gestureSettled: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            windows: [], transfers: [], effects: [], result: null,
        }));
        const auto = lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        auto.callback(accepted(gen, autoBody.source, autoBody.nativeInvalid, "0"));
    };
    const winRow = (
        bare: string,
        frame: [number, number, number, number] | null,
        auth: "controlled" | "untouched",
        extra: Record<string, unknown> = {},
    ): Record<string, unknown> => ({
        win: bare, out: OUT, ws: WS_A,
        alloc: auth === "untouched" ? null : frame, frame,
        frame_auth: auth, frame_hold: "",
        member_auth: auth, member_hold: "",
        frame_op: "",
        clear_op: "",
        floating: auth === "untouched", float_origin: "",
        ...extra,
    });
    const settleAsync = (): void => {
        const st = live.get(WIN_A);
        assert.ok(st !== undefined);
        st.maximizeMode = 0;
    };
    // Seed intentional-float membership for WIN_A through an accepted
    // untouched row (production convergence path, no fixture shortcut).
    const seedFloat = (rev: string): string => {
        handle?.requestFloat();
        runTimers();
        const call = lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows: [winRow(WIN_A, FLOAT_A, "untouched"), winRow(WIN_B, TILE_B, "controlled")],
            result: { op: "toggle-float", win: WIN_A, rect: FLOAT_A },
        }));
        // The reply places the float; mirror the applied placement into the
        // live store so later unfloat legs carry the floating frame (the
        // apply path writes through the wrapper setters on the live host;
        // the synthetic wrapper snapshot here needs the same end state).
        const placed = live.get(WIN_A);
        assert.ok(placed !== undefined);
        placed.frame = { x: FLOAT_A[0], y: FLOAT_A[1], width: FLOAT_A[2], height: FLOAT_A[3] };
        return String((JSON.parse(call.payload) as Record<string, unknown>)["req"]);
    };
    return {
        calls, logs, geometryWrites, handle: handle as NonNullable<typeof handle>,
        runTimers, lastCall, accepted, discover, winRow, settleAsync, seedFloat,
        live,
        setActive: (bare: string) => {
            activeBare = bare;
        },
        setMaximized: (bare: string, modeValue: number) => {
            const st = live.get(bare);
            assert.ok(st !== undefined);
            st.maximizeMode = modeValue;
        },
        setFullscreen: (bare: string, value: boolean) => {
            const st = live.get(bare);
            assert.ok(st !== undefined);
            st.fullScreen = value;
        },
        removeLive: (bare: string) => {
            live.delete(bare);
        },
        setWs: (bare: string, ws: string) => {
            const st = live.get(bare);
            assert.ok(st !== undefined);
            st.ws = ws;
        },
        addDesktop: (id: string) => {
            desktops.push({ id });
        },
        setGesture: (value: boolean) => {
            gesture = value;
        },
        commandOf: (payload: string): Record<string, unknown> | null => {
            const parsed = JSON.parse(payload) as Record<string, unknown>;
            const cmd = parsed["command"] as Record<string, unknown> | undefined;
            return cmd ?? null;
        },
    };
}

describe("world B9 deferred maximized unfloat", () => {
    const toggleCount = (h: ReturnType<typeof makeHarness>): number =>
        h.calls
            .filter((c) => c.method === "QueueWorldCommand")
            .map((c) => h.commandOf(c.payload))
            .filter((cmdVal) => cmdVal !== null && (cmdVal["op"] as unknown) === "operation")
            .filter((op) => ((op as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] === "toggle-float")
            .length;

    const boundaryReply = (h: ReturnType<typeof makeHarness>, rev: string): void => {
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        assert.ok(!("command" in (body["report"] as Record<string, unknown>)), "clear schedules observation first");
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows: [h.winRow(WIN_A, FLOAT_A, "untouched"), h.winRow(WIN_B, TILE_B, "controlled")],
            result: null,
        }));
    };

    it("emits no same-stack command: sync settlement completes on the next report", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        // Same-stack: the clear issued and armed, but no toggle-float
        // command rides the current stack.
        assert.ok(h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")));
        const syncCalls = h.calls.filter((c) => c.method === "QueueWorldCommand");
        const syncCommands = syncCalls
            .map((c) => h.commandOf(c.payload))
            .filter((cmd) => cmd !== null && (cmd["op"] as unknown) === "operation");
        assert.equal(syncCommands.length, 1, "only the seed float command exists synchronously");
        assert.ok(!h.logs.some((l) => l.includes("outcome=observed-cleared")), "no same-stack admission");
        // Next report observes the cleared mode and admits exactly once.
        boundaryReply(h, "2");
        h.runTimers();
        h.runTimers();
        const admittedQ = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = admittedQ["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        assert.deepEqual((cmd["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.deepEqual((cmd["command"] as Record<string, unknown>)["rect"], [...FLOAT_A]);
        assert.ok(h.logs.some((l) => l.includes("outcome=observed-cleared")), "deferred admission after observed clear");
        assert.equal(toggleCount(h), 2, "seed float plus exactly one deferred unfloat command");
    });

    it("terminates on the first report when settlement is still pending, later press retries", () => {
        const h = makeHarness("async-manual");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        assert.ok(h.logs.some((l) => l.includes("outcome=issued")));
        boundaryReply(h, "2");
        // Still maximized on the first fresh report: the attempt ends
        // here; no indefinite wait for a later restore.
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=still-maximized")));
        assert.equal(toggleCount(h), 1, "unconfirmed clear never admits");
        // Late live settlement plus an explicit later press: the window now
        // reads unmaximized, so the retry is an ordinary immediate unfloat.
        h.settleAsync();
        h.handle.requestFloat();
        h.runTimers();
        const retryQ = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const retryCmd = retryQ["command"] as Record<string, unknown>;
        assert.equal((retryCmd["command"] as Record<string, unknown>)["op"], "toggle-float");
        assert.deepEqual((retryCmd["command"] as Record<string, unknown>)["rect"], [...FLOAT_A]);
        assert.equal(toggleCount(h), 2, "seed float plus exactly one retried unfloat command");
    });

    it("terminates a silent stubborn clear on the first report", () => {
        const h = makeHarness("stubborn");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        boundaryReply(h, "2");
        // No signal, no settlement, no second chance: one boundary.
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=still-maximized")));
        assert.equal(toggleCount(h), 1, "raced clear never admits");
    });

    it("cancels on a raced focus change with no command", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.setActive(WIN_B);
        boundaryReply(h, "2");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=focus-raced")));
        assert.equal(toggleCount(h), 1, "only the seed float command");
    });

    it("cancels when the lifetime leaves the inventory", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.removeLive(WIN_A);
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: [h.winRow(WIN_B, TILE_B, "controlled")],
            result: null,
        }));
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=observe-missing")));
        assert.equal(toggleCount(h), 1, "only the seed float command");
    });

    it("cancels when fullscreen races in before the boundary", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.setFullscreen(WIN_A, true);
        boundaryReply(h, "2");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=fullscreen-raced")));
        assert.equal(toggleCount(h), 1, "only the seed float command");
    });

    it("cancels on a scope race with no command", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.addDesktop(WS_B);
        h.setWs(WIN_A, WS_B);
        boundaryReply(h, "2");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=scope-raced")));
        assert.equal(toggleCount(h), 1, "moved scope never admits");
    });

    it("cancels mid-gesture with no command", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.setGesture(true);
        boundaryReply(h, "2");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=gesture-raced")));
        assert.equal(toggleCount(h), 1, "only the seed float command");
    });

    it("cancels on newer explicit input before the boundary", () => {
        const h = makeHarness("async-manual");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        assert.ok(h.logs.some((l) => l.includes("outcome=issued")));
        // A newer explicit resize supersedes the pending attempt at
        // submit time (no later completion).
        h.handle.requestResize("left", "outwards");
        assert.ok(h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=newer-input-superseded")));
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        const resizeQ = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = resizeQ["command"] as Record<string, unknown>;
        assert.equal((cmd["command"] as Record<string, unknown>)["op"], "resize");
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: [h.winRow(WIN_A, FLOAT_A, "untouched"), h.winRow(WIN_B, TILE_B, "controlled")],
            result: null,
        }));
        assert.equal(toggleCount(h), 1, "superseded attempt never completes");
        assert.ok(!h.logs.some((l) => l.includes("outcome=observed-cleared")));
    });

    it("never replays the clear across a generation rebind", () => {
        const h = makeHarness("async-manual");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        // Poison generation: the attempt terminates, discovery rebinds,
        // and no toggle-float rides any generation.
        call.callback(JSON.stringify({
            v: 3, outcome: "rejected", kind: "generation-mismatch", message: "poison", gen: "gen-2",
            source: (JSON.parse(call.payload) as Record<string, unknown>)["source"],
            nativeInvalid: (JSON.parse(call.payload) as Record<string, unknown>)["nativeInvalid"],
            clock: "0",
        }));
        assert.ok(h.logs.some((l) => l.includes("generation-mismatch")));
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=generation-mismatch")));
        assert.equal(toggleCount(h), 1, "no replay across rebind");
        // The rebind handshake completes cleanly with no pending attempt.
        const disc = h.lastCall("DiscoverWorld");
        const discBody = JSON.parse(disc.payload);
        disc.callback(JSON.stringify({
            v: 3, adapter: "broker-1", gen: "gen-2", req: discBody.req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            gestureSettled: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            windows: [], transfers: [], effects: [], result: null,
        }));
        h.runTimers();
        const auto = h.lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload) as Record<string, unknown>;
        auto.callback(h.accepted("gen-2", autoBody["source"] as string, autoBody["nativeInvalid"] as string, "3", {
            windows: [h.winRow(WIN_A, FLOAT_A, "untouched"), h.winRow(WIN_B, TILE_B, "controlled")],
            result: null,
        }));
        assert.equal(toggleCount(h), 1, "rebind settles with no admission");
    });

    it("terminates on transport failure with no later admission", () => {
        const h = makeHarness("async-manual");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        // Non-string reply: transport failure ends the attempt.
        call.callback(42);
        assert.ok(h.logs.some((l) => l.includes("transport-error")));
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=transport")));
        assert.equal(toggleCount(h), 1, "failed flight never admits");
    });

    it("refuses a missing setter with no pending state", () => {
        const h = makeHarness("missing");
        h.discover();
        h.seedFloat("1");
        h.setMaximized(WIN_A, 3);
        h.handle.requestFloat();
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=native-write-refused")));
        assert.equal(toggleCount(h), 1, "no pending state left behind");
    });

    it("keeps the ordinary immediate unfloat path unchanged", () => {
        const h = makeHarness("sync");
        h.discover();
        h.seedFloat("1");
        // Ordinary floating, non-maximized unfloat submits synchronously.
        h.handle.requestFloat();
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        JSON.parse(call.payload);
        const immQ = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = immQ["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        assert.deepEqual(cmd["command"], { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] });
        assert.ok(!h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")));
    });
});
