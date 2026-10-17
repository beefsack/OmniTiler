// Regression: Meta+G unfloat must re-tile immediately on the full-state
// reply route, with no focus/event between the reply and the frame write.
// The production entry (`startWorldAdapterEntry`) drives the whole
// float -> unfloat sequence below: the float leg places the retained frame,
// the unfloat leg must write the tile frame synchronously inside the reply
// callback. A repeat snapshot must be a no-op, native overlays stay
// guarded, and a fixed-size unfloat (tile-override addressed) retile as
// well. Host shapes mirror the live producer: QML array-likes (never JS
// arrays), braced `{uuid}` live ids with bare ids on the wire, real KWin
// property names, and Qt zero/unbounded size hints for ordinary windows.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

const WS_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const WIN_F = "44444444-4444-4444-4444-444444444444";
const WIN_O = "55555555-5555-5555-5555-555555555555";
const OUT = "out-1";

// Qt producer hint shapes: ordinary terminals carry a zero minimum and the
// unbounded QWidget maximum sentinel; only an exactly-equal finite min/max
// pair classifies fixed-size.
const QT_ZERO = { width: 0, height: 0 };
const QT_UNBOUNDED = { width: 16777215, height: 16777215 };
const FIXED_HINT = { width: 480, height: 320 };

const TILE_A: [number, number, number, number] = [0, 0, 400, 600];
const TILE_B_HALF: [number, number, number, number] = [400, 0, 400, 600];
const TILE_B_FULL: [number, number, number, number] = [0, 0, 800, 600];
const FLOAT_A: [number, number, number, number] = [162, 122, 475, 355];
const TILE_F: [number, number, number, number] = [0, 0, 400, 600];
const FLOAT_F: [number, number, number, number] = [100, 100, 480, 320];

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

interface LiveSpec {
    braced: string;
    ws: string;
    frame: { x: number; y: number; width: number; height: number };
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number };
    maxSize: { width: number; height: number };
}

function makeHarness(initial: LiveSpec[]) {
    const live = new Map<string, LiveSpec>();
    for (const w of initial) {
        live.set(w.braced, { ...w, frame: { ...w.frame } });
    }
    const geometryWrites: string[] = [];
    let activeBraced: string | null = initial.length > 0 ? (initial[0] as LiveSpec).braced : null;
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
    const wrap = (spec: LiveSpec): object => {
        const w: Record<string, unknown> = {
            internalId: spec.braced,
            normalWindow: true,
            output: { name: OUT },
            desktops: qmlLike([{ id: spec.ws }]),
            minimized: spec.minimized,
            fullscreen: spec.fullScreen,
            maximized: spec.maximizeMode !== 0,
            sticky: spec.onAllDesktops,
            keepAbove: false,
            keepBelow: false,
            minSize: { ...spec.minSize },
            maxSize: { ...spec.maxSize },
            fullScreen: spec.fullScreen,
            maximizeMode: spec.maximizeMode,
            onAllDesktops: spec.onAllDesktops,
        };
        let frame = { ...spec.frame };
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => ({ ...frame }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = { ...rect };
                const cur = live.get(spec.braced);
                if (cur !== undefined) {
                    cur.frame = { ...rect };
                }
                geometryWrites.push(`${spec.braced}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        return w;
    };
    const surface = {
        windowList: () => qmlLike([...live.values()].map((s) => wrap(s))),
        get screens() {
            return qmlLike(screens.map((s) => ({ ...s })));
        },
        get desktops() {
            return qmlLike(desktops.map((d) => ({ ...d })));
        },
        activeScreen: { name: OUT },
        get activeWindow() {
            return activeBraced === null ? null : { internalId: activeBraced };
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
    const accepted = (
        gen: string,
        req: string,
        obs: string,
        rev: string,
        extra: Record<string, unknown> = {},
    ): string =>
        JSON.stringify({
            v: 1, adapter: "test-adapter", gen, req, obs, rev,
            ready: true, consumed: false,
            scopes: [{ scope: `${OUT}/${WS_A}`, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [], windows: [], transfers: [], effects: [],
            ...extra,
        });
    const discover = (gen = "gen-1"): void => {
        const call = lastCall("DiscoverWorld");
        const body = JSON.parse(call.payload);
        call.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen, req: body.req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            windows: [], transfers: [], effects: [],
        }));
        const auto = lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        auto.callback(accepted(gen, autoBody.req, autoBody.obs, "0"));
    };
    const fire = (name: keyof typeof signals, payload?: unknown): void => {
        for (const h of [...(signals[name] as { handlers: Handler[] }).handlers]) {
            (h as (p?: unknown) => void)(payload);
        }
    };
    const liveFrame = (bare: string): { x: number; y: number; width: number; height: number } => {
        const spec = live.get(`{${bare}}`);
        assert.ok(spec !== undefined, `live window ${bare}`);
        return { ...(spec as LiveSpec).frame };
    };
    return {
        calls, logs, signals, geometryWrites, handle, runTimers, lastCall, discover, accepted, fire, liveFrame,
        nonNullHandle: handle as NonNullable<typeof handle>,
        frameOf: (bare: string): [number, number, number, number] => {
            const f = liveFrame(bare);
            return [f.x, f.y, f.width, f.height];
        },
    };
}

const termSpec = (
    bare: string,
    frame: { x: number; y: number; width: number; height: number },
    extra: Partial<LiveSpec> = {},
): LiveSpec => ({
    braced: `{${bare}}`,
    ws: WS_A,
    frame,
    minimized: false,
    fullScreen: false,
    maximizeMode: 0,
    onAllDesktops: false,
    minSize: { ...QT_ZERO },
    maxSize: { ...QT_UNBOUNDED },
    ...extra,
});

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
    ...extra,
});

describe("world unfloat immediate retile", () => {
    it("float places then unfloat retiles synchronously with no focus event between", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        // Float leg: Meta+G on the active terminal carries no rect; the
        // reply carries the retained placement token plus the sibling reflow.
        entry.requestFloat();
        h.runTimers();
        let call = h.lastCall("DescribeWorld");
        let body = JSON.parse(call.payload) as Record<string, unknown>;
        assert.deepEqual(
            (body["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: null },
        );
        call.callback(h.accepted("gen-1", body["req"] as string, body["obs"] as string, "1", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1" }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: FLOAT_A },
        }));
        assert.deepEqual(h.frameOf(WIN_A), FLOAT_A, "float places the retained frame");
        assert.deepEqual(h.frameOf(WIN_B), TILE_B_FULL, "sibling reflows into the gap");
        const writesAfterFloat = h.geometryWrites.length;
        assert.ok(writesAfterFloat >= 2, "both frames written on float");

        // Unfloat leg: Meta+G again carries the live floating frame. The
        // reply must move the toggled window back to its tile inside the
        // reply callback itself: no fire(), no runTimers(), no focus change.
        entry.requestFloat();
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        assert.deepEqual(
            (body["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        );
        call.callback(h.accepted("gen-1", body["req"] as string, body["obs"] as string, "2", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled"),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: null },
        }));
        assert.deepEqual(h.frameOf(WIN_A), TILE_A, "unfloat retiles synchronously without a focus event");
        assert.deepEqual(h.frameOf(WIN_B), TILE_B_HALF, "sibling returns to its half");
        const writesAfterUnfloat = h.geometryWrites.length;
        assert.ok(writesAfterUnfloat > writesAfterFloat, "unfloat writes frames");

        // Repeat snapshot converges: a later identical desired state with no
        // native change writes nothing.
        h.fire("windowActivated");
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        assert.equal(body["command"], null, "repeat is observation-only");
        call.callback(h.accepted("gen-1", body["req"] as string, body["obs"] as string, "3", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled"),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
            result: null,
        }));
        assert.equal(h.geometryWrites.length, writesAfterUnfloat, "converged repeat writes nothing");
        assert.deepEqual(h.frameOf(WIN_A), TILE_A);
    });

    it("fixed-size unfloat retiles through the tile override without a focus event", () => {
        const h = makeHarness([
            {
                ...termSpec(WIN_F, { x: FLOAT_F[0], y: FLOAT_F[1], width: FLOAT_F[2], height: FLOAT_F[3] }),
                minSize: { ...FIXED_HINT },
                maxSize: { ...FIXED_HINT },
            },
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        // The fixed window opens auto-floated: an observation-only reply
        // carries the untouched membership (ordinary float, no placement
        // token since the live frame already matches), converging the
        // adapter float set through the production path.
        h.fire("windowActivated");
        h.runTimers();
        let seed = h.lastCall("DescribeWorld");
        let seedBody = JSON.parse(seed.payload) as Record<string, unknown>;
        assert.equal(seedBody["command"], null);
        seed.callback(h.accepted("gen-1", seedBody["req"] as string, seedBody["obs"] as string, "1", {
            windows: [winRow(WIN_F, FLOAT_F, "untouched")],
            result: null,
        }));
        assert.deepEqual(h.frameOf(WIN_F), FLOAT_F);

        entry.requestFloat();
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        assert.deepEqual(
            (body["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_F, rect: [...FLOAT_F] },
        );
        const writesBefore = h.geometryWrites.length;
        call.callback(h.accepted("gen-1", body["req"] as string, body["obs"] as string, "2", {
            windows: [winRow(WIN_F, TILE_F, "controlled")],
            result: { op: "toggle-float", win: WIN_F, rect: null },
        }));
        assert.ok(h.geometryWrites.length > writesBefore, "fixed-size unfloat writes the tile frame");
        // This host accepts the requested frame; native clients may clamp it.
        // The regression signal is the immediate write attempt above.
        assert.deepEqual(h.frameOf(WIN_F), TILE_F, "accepted fixed-size tile frame");

        h.fire("windowActivated");
        h.runTimers();
        const repeat = h.lastCall("DescribeWorld");
        const repeatBody = JSON.parse(repeat.payload) as Record<string, unknown>;
        repeat.callback(h.accepted("gen-1", repeatBody["req"] as string, repeatBody["obs"] as string, "3", {
            windows: [winRow(WIN_F, TILE_F, "controlled")],
            result: null,
        }));
        assert.deepEqual(h.frameOf(WIN_F), TILE_F, "fixed-size tile sticks on repeat");
    });

    it("native fullscreen overlay still takes no automatic frame write", () => {
        const live: LiveSpec = {
            ...termSpec(WIN_O, { x: 0, y: 0, width: 100, height: 100 }),
            fullScreen: true,
        };
        const h = makeHarness([live]);
        h.discover();
        h.fire("windowActivated");
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        const writesBefore = h.geometryWrites.length;
        call.callback(h.accepted("gen-1", body["req"] as string, body["obs"] as string, "1", {
            windows: [winRow(WIN_O, TILE_A, "controlled")],
            result: null,
        }));
        assert.equal(h.geometryWrites.length, writesBefore, "overlay frame never auto-writes");
        assert.deepEqual(h.frameOf(WIN_O), [0, 0, 100, 100]);
    });
});
