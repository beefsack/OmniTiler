// Regression: production world entry explicit close with QML-like host and
// braced UUIDs. Live wrappers carry `{uuid}`, reports carry bare ids. The
// removal payload is never read (dead QObject): closes come from the
// event-gated full-inventory difference only.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

const WS_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WS_B = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const WIN_C = "33333333-3333-3333-3333-333333333333";
const OUT = "out-1";

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

interface LiveWin {
    braced: string;
    ws: string;
    minimized: boolean;
}

function makeHarness() {
    const desktops = [{ id: WS_A }, { id: WS_B }];
    const screens = [{ name: OUT, geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    let live: LiveWin[] = [
        { braced: `{${WIN_A}}`, ws: WS_A, minimized: false },
        { braced: `{${WIN_B}}`, ws: WS_B, minimized: false },
    ];
    let activeBraced: string | null = `{${WIN_A}}`;
    let listBroken = false;
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
    const freshWrappers = (): object =>
        qmlLike(
            live.map((w) => ({
                internalId: w.braced,
                normalWindow: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: OUT },
                desktops: qmlLike([{ id: w.ws }]),
                minimized: w.minimized,
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
            })),
        );
    const surface = {
        windowList: () => {
            if (listBroken) {
                throw new Error("unreadable-list");
            }
            return freshWrappers();
        },
        get screens() {
            return qmlLike(screens.map((s) => ({ ...s })));
        },
        get desktops() {
            return qmlLike(desktops.map((d) => ({ ...d })));
        },
        activeScreen: { name: OUT },
        get activeWindow() {
            if (activeBraced === null) {
                return null;
            }
            return { internalId: activeBraced };
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
                callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                }            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => {
            logs.push(m);
        },
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
    const lastCall = (method: string) => {
        const found = [...calls].reverse().find((c) => c.method === method);
        assert.ok(found !== undefined, `expected ${method}`);
        return found as { method: string; payload: string; callback: (reply: unknown) => void };
    };
    const accepted = (gen: string, source: string, nativeInvalid: string, rev: string): string =>
        JSON.stringify({
            v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev,
            ready: true, consumed: false,
            source,
            nativeInvalid,
            clock: "0",
            gestureSettled: null,
            operationSettled: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [{ scope: `${OUT}/${WS_A}`, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
        });
    const discover = (gen = "gen-1"): void => {
        const call = lastCall("DiscoverWorld");
        const body = JSON.parse(call.payload);
        call.callback(JSON.stringify({
            v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            gestureSettled: null,
            operationSettled: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            windows: [], transfers: [], effects: [], result: null,
        }));
        const auto = lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        auto.callback(accepted(gen, autoBody.source, autoBody.nativeInvalid, "0"));
    };
    const requestPayload = (): Record<string, unknown> =>
        JSON.parse(lastCall("DescribeWorld").payload) as Record<string, unknown>;
    const fire = (name: keyof typeof signals, payload?: unknown): void => {
        for (const h of [...(signals[name] as { handlers: Handler[] }).handlers]) {
            (h as (p?: unknown) => void)(payload);
        }
    };
    return {
        calls, logs, signals, handle, runTimers, lastCall, discover, accepted, requestPayload, fire,
        setLive: (next: LiveWin[]): void => {
            live = next;
        },
        getLive: (): LiveWin[] => live,
        setActive: (braced: string | null): void => {
            activeBraced = braced;
        },
        setListBroken: (broken: boolean): void => {
            listBroken = broken;
        },
    };
}

describe("world close braced qml regression", () => {
    it("observes added generic windows and closes braced ids without reading the payload", () => {
        const h = makeHarness();
        h.discover();
        // Add an owned generic window while live: it observes under its bare id.
        h.setLive([
            { braced: `{${WIN_A}}`, ws: WS_A, minimized: false },
            { braced: `{${WIN_B}}`, ws: WS_B, minimized: false },
            { braced: `{${WIN_C}}`, ws: WS_A, minimized: false },
        ]);
        h.fire("windowAdded", { internalId: `{${WIN_C}}` });
        (h.handle as NonNullable<typeof h.handle>).requestWorkspaceRelative(1);
        h.runTimers();
        let req = h.requestPayload();
        const inner = req["report"] as Record<string, unknown>;
        let wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.ok(wins.some((w) => w["win"] === WIN_C), "added generic window observes as bare id");
        h.lastCall("DescribeWorld").callback(h.accepted("gen-1", req["source"] as string, "1", "1"));
        // Minimize WIN_A but keep it live: it must never read as closed.
        h.setLive([
            { braced: `{${WIN_A}}`, ws: WS_A, minimized: true },
            { braced: `{${WIN_B}}`, ws: WS_B, minimized: false },
            { braced: `{${WIN_C}}`, ws: WS_A, minimized: false },
        ]);
        // Remove WIN_B with a payload that throws on any property access and
        // a fresh wrapper identity (never the live ref). The entry must not
        // touch the payload at all.
        let getterCalls = 0;
        const evil: object = {};
        Object.defineProperty(evil, "internalId", {
            configurable: true,
            get: (): string => {
                getterCalls += 1;
                throw new Error("dead-qobject");
            },
        });
        h.setLive([
            { braced: `{${WIN_A}}`, ws: WS_A, minimized: true },
            { braced: `{${WIN_C}}`, ws: WS_A, minimized: false },
        ]);
        h.setActive(`{${WIN_C}}`);
        h.fire("windowRemoved", evil);
        // A duplicate removal event for the same close must not duplicate it.
        h.fire("windowRemoved", {});
        (h.handle as NonNullable<typeof h.handle>).requestWorkspaceRelative(1);
        h.runTimers();
        req = h.requestPayload();
        const inner2 = req["report"] as Record<string, unknown>;
        const closed = inner2["closed"] as Array<string>;
        assert.deepEqual(closed, [WIN_B], "closed carries the bare uuid exactly once");
        assert.equal(getterCalls, 0, "removed payload getters never run");
        const facts = inner2["windows"] as Array<Record<string, unknown>>;
        assert.ok(facts.some((m) => m["win"] === WIN_A && m["minimized"] === true), "minimized live window rides minimized");
        assert.ok(!closed.includes(WIN_A), "minimized live window never closes");
        wins = facts;
        assert.ok(!wins.some((w) => (w as Record<string, unknown>)["win"] === WIN_B));
        assert.ok(wins.some((w) => (w as Record<string, unknown>)["win"] === WIN_C));
        // Unknown-ref removal erases nothing: the retained close survives.
        // (Do not ack yet; the flight is still open so queue the event and
        // check after the ack round-trip below retains-then-clears.)
        h.lastCall("DescribeWorld").callback(h.accepted("gen-1", req["source"] as string, "1", "2"));
        // After ack the close clears on the next report.
        (h.handle as NonNullable<typeof h.handle>).requestWorkspaceRelative(1);
        h.runTimers();
        const after = h.requestPayload();
        const afterClosed = (after["report"] as Record<string, unknown>)["closed"] as Array<string>;
        assert.deepEqual(afterClosed, [], "acked close clears");
        h.lastCall("DescribeWorld").callback(h.accepted("gen-1", after["source"] as string, "1", "3"));
        // Unknown-ref removal with no live difference fabricates nothing.
        h.fire("windowRemoved", {});
        (h.handle as NonNullable<typeof h.handle>).requestWorkspaceRelative(1);
        h.runTimers();
        const unknown = h.requestPayload();
        const unknownClosed = (unknown["report"] as Record<string, unknown>)["closed"] as Array<string>;
        assert.deepEqual(unknownClosed, [], "unknown removal fabricates no close");
    });

    it("defers the close when the full inventory is unreadable", () => {
        const h = makeHarness();
        h.discover();
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.setListBroken(true);
        h.fire("windowRemoved", {});
        (h.handle as NonNullable<typeof h.handle>).requestWorkspaceRelative(1);
        h.runTimers();
        // Unreadable inventory refuses the report instead of inventing a close.
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, describesBefore);
        assert.ok(h.logs.some((l) => l.includes("observe-invalid")));
        // Recovery still converges once the list reads again (no close was
        // fabricated for the unreadable window). The failed dispatch keeps
        // its command, so recovery rides the next native event.
        h.setListBroken(false);
        h.fire("windowActivated");
        h.runTimers();
        assert.ok(h.calls.filter((c) => c.method === "DescribeWorld").length > describesBefore);
    });
});
