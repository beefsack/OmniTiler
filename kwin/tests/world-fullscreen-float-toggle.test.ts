// Fullscreen float toggle at the production entry boundary (D5):
// explicit Meta+G on a tiled fullscreen subject changes base membership
// immediately (siblings reflow) with zero writes to the fullscreen
// subject (native state, geometry, focus, stacking); a second toggle
// fresh-admits invisibly; exiting fullscreen reveals the durable retained
// placement (centered 60% fallback) or the tile slot. Maximized still
// refuses via core (adapter submits with zero writes, B9 never arms under
// fullscreen). Host shapes mirror the live producer: QML array-likes
// (never JS arrays), braced `{uuid}` live ids with bare ids on the wire,
// renewed wrappers per enumeration (never ref identity), real KWin
// property names, and Qt zero/unbounded size hints for ordinary windows.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

const WS_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const OUT = "out-1";

// Qt producer hint shapes: ordinary terminals carry a zero minimum and the
// unbounded QWidget maximum sentinel; only an exactly-equal finite min/max
// pair classifies fixed-size.
const QT_ZERO = { width: 0, height: 0 };
const QT_UNBOUNDED = { width: 16777215, height: 16777215 };

// Fullscreen native frame (full client area) is distinct from every
// retained/tile frame below, so any capture of it is observable.
const FULLSCREEN_FRAME: [number, number, number, number] = [0, 0, 800, 600];
const TILE_A: [number, number, number, number] = [0, 0, 400, 600];
const TILE_B_HALF: [number, number, number, number] = [400, 0, 400, 600];
const TILE_B_FULL: [number, number, number, number] = [0, 0, 800, 600];
// Centered 60% fallback for the 800x600 work area (durable placement when
// the window has never floated before).
const FLOAT_A: [number, number, number, number] = [160, 120, 480, 360];

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
    const focusWrites: string[] = [];
    const aboveWrites: string[] = [];
    const nativeStateWrites: string[] = [];
    // Applied-flag lag hook: a stuck window records requested native
    // setters in call order but its script getters keep returning the
    // pre-write applied values (Wayland client-ack lag).
    const appliedStuck = new Set<string>();
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
    const screens = [{ name: OUT, geometry: { x: 0, y: 0, width: 800, height: 600 } }];
    const wrap = (spec: LiveSpec): object => {
        const stuckNow = appliedStuck.has(spec.braced);
        const w: Record<string, unknown> = {
            internalId: spec.braced, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            output: { name: OUT },
            desktops: qmlLike([{ id: spec.ws }]),
            minimized: spec.minimized,
            hidden: false,
            keepBelow: false,
            minSize: { ...spec.minSize },
            maxSize: { ...spec.maxSize },
        };
        let fullScreen = spec.fullScreen;
        let maximizeMode = spec.maximizeMode;
        let onAllDesktops = spec.onAllDesktops;
        const syncLive = (): void => {
            const cur = live.get(spec.braced);
            if (cur !== undefined) {
                cur.fullScreen = fullScreen;
                cur.maximizeMode = maximizeMode;
                cur.onAllDesktops = onAllDesktops;
            }
        };
        const recordNative = (prop: string, value: unknown): void => {
            nativeStateWrites.push(`${spec.braced}@${prop}=${String(value)}`);
        };
        Object.defineProperty(w, "fullScreen", {
            configurable: true,
            get: () => (stuckNow ? spec.fullScreen : fullScreen),
            set: (v: boolean) => {
                recordNative("fullScreen", v);
                if (!stuckNow) {
                    fullScreen = v;
                    syncLive();
                }
                return true;
            },
        });
        Object.defineProperty(w, "fullscreen", {
            configurable: true,
            get: () => (stuckNow ? spec.fullScreen : fullScreen),
            set: (v: boolean) => {
                recordNative("fullscreen", v);
                if (!stuckNow) {
                    fullScreen = v;
                    syncLive();
                }
                return true;
            },
        });
        Object.defineProperty(w, "maximizeMode", {
            configurable: true,
            get: () => (stuckNow ? spec.maximizeMode : maximizeMode),
            set: (v: number) => {
                recordNative("maximizeMode", v);
                if (!stuckNow) {
                    maximizeMode = v;
                    syncLive();
                }
                return true;
            },
        });
        Object.defineProperty(w, "maximized", {
            configurable: true,
            get: () => (stuckNow ? spec.maximizeMode !== 0 : maximizeMode !== 0),
            set: (v: boolean) => {
                recordNative("maximized", v);
                if (!stuckNow) {
                    maximizeMode = v ? 3 : 0;
                    syncLive();
                }
                return true;
            },
        });
        Object.defineProperty(w, "onAllDesktops", {
            configurable: true,
            get: () => onAllDesktops,
            set: (v: boolean) => {
                onAllDesktops = v;
                syncLive();
                recordNative("onAllDesktops", v);
                return true;
            },
        });
        Object.defineProperty(w, "sticky", {
            configurable: true,
            get: () => onAllDesktops,
            set: (v: boolean) => {
                onAllDesktops = v;
                syncLive();
                recordNative("sticky", v);
                return true;
            },
        });
        w["setMaximize"] = (vertically: boolean, horizontally: boolean): void => {
            nativeStateWrites.push(`${spec.braced}@setMaximize:${vertically},${horizontally}`);
            if (!stuckNow) {
                maximizeMode = vertically && horizontally ? 3 : 0;
                syncLive();
            }
        };
        let above = false;
        Object.defineProperty(w, "keepAbove", {
            configurable: true,
            get: () => above,
            set: (v: boolean) => {
                above = v;
                aboveWrites.push(`${spec.braced}@${v}`);
                return true;
            },
        });
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
    const surface: Record<string, unknown> = {
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
        set activeWindow(ref: unknown) {
            try {
                const id = (ref as { internalId?: unknown })?.internalId;
                focusWrites.push(String(id ?? "null"));
                if (typeof id === "string") {
                    activeBraced = id;
                }
            } catch (error) {
                void error;
            }
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
                }
            }
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
        source: string,
        nativeInvalid: string,
        rev: string,
        extra: Record<string, unknown> = {},
    ): string => {
        const { result, ...snapshot } = extra;
        if (result !== undefined) {
            (globalThis as Record<string, unknown>)["__takeResult"] = result;
        }
        return JSON.stringify({
            v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev,
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
            v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
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
    const fire = (name: keyof typeof signals, payload?: unknown): void => {
        for (const h of [...(signals[name] as { handlers: Handler[] }).handlers]) {
            (h as (p?: unknown) => void)(payload);
        }
    };
    const subjectWrites = (braced: string): string[] =>
        geometryWrites.filter((w) => w.startsWith(`${braced}@`));
    const subjectNativeWrites = (braced: string): string[] =>
        nativeStateWrites.filter((w) => w.startsWith(`${braced}@`));
    return {
        calls, logs, signals, geometryWrites, focusWrites, aboveWrites, nativeStateWrites,
        handle, runTimers, lastCall, discover, accepted, fire, subjectWrites, subjectNativeWrites,
        nonNullHandle: handle as NonNullable<typeof handle>,
        live,
        setFullscreen: (bare: string, value: boolean): void => {
            const st = live.get(`{${bare}}`);
            assert.ok(st !== undefined);
            st.fullScreen = value;
        },
        setMaximized: (bare: string, modeValue: number): void => {
            const st = live.get(`{${bare}}`);
            assert.ok(st !== undefined);
            st.maximizeMode = modeValue;
        },
        setActive: (bare: string): void => {
            activeBraced = `{${bare}}`;
        },
        stickApplied: (bare: string): void => {
            appliedStuck.add(`{${bare}}`);
        },
        liveFrame: (bare: string): [number, number, number, number] => {
            const spec = live.get(`{${bare}}`);
            assert.ok(spec !== undefined, `live window ${bare}`);
            return [spec.frame.x, spec.frame.y, spec.frame.width, spec.frame.height];
        },
        commandOf: (payload: string): Record<string, unknown> | null => {
            const parsed = JSON.parse(payload) as Record<string, unknown>;
            return (parsed["command"] as Record<string, unknown> | undefined) ?? null;
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
    clear_op: "",
    floating: false, float_origin: "",
    ...extra,
});

describe("world fullscreen float toggle", () => {
    it("single toggle floats beneath retained fullscreen with zero subject writes", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        // Explicit Meta+G on the tiled fullscreen subject carries no rect:
        // the session owns the retained placement (centered 60% fallback).
        entry.requestFloat();
        h.runTimers();
        let call = h.lastCall("DescribeWorld");
        let body = JSON.parse(call.payload) as Record<string, unknown>;
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(
            (filed["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: null },
            "fullscreen float carries no rect, never the fullscreen frame",
        );
        // Core admits beneath the retained overlay: the float row carries
        // the retained placement token, the sibling reflows into the gap.
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        }));
        // D5: zero writes to the fullscreen subject (geometry, focus,
        // stacking, native state); the sibling still reflows behind it.
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME, "fullscreen subject frame untouched");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_FULL, "sibling reflows into the released tile");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write targets the fullscreen subject");
        assert.ok(h.geometryWrites.length >= 1, "sibling reflow writes");
        assert.deepEqual(h.focusWrites.filter((f) => f.includes(WIN_A)), [], "no focus write targets the fullscreen subject");
        assert.deepEqual(h.aboveWrites.filter((w) => w.startsWith(`{${WIN_A}}`)), [], "no keepAbove write targets the fullscreen subject");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "no native state write targets the fullscreen subject");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, true, "native fullscreen state untouched");
        assert.equal(spec?.maximizeMode, 0, "native maximize state untouched");
        assert.equal(spec?.onAllDesktops, false, "native sticky state untouched");
    });

    it("double toggle fresh-admits invisibly with zero subject writes and keeps the retained frame", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestFloat();
        h.runTimers();
        let call = h.lastCall("DescribeWorld");
        let body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        }));
        const writesAfterFloat = h.geometryWrites.length;

        // Second toggle on the floating fullscreen subject carries no
        // rect: the observed fullscreen frame never becomes retained.
        entry.requestFloat();
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        const filedUnfloat = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(
            (filedUnfloat["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: null },
            "fullscreen unfloat carries no rect, never the fullscreen frame",
        );
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled"),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: null },
        }));
        // Fresh admission is invisible behind retained fullscreen: the
        // subject takes no geometry write, the sibling returns to half.
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME, "fresh admission writes nothing to the fullscreen subject");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_HALF, "sibling returns to its half");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write targets the fullscreen subject on unfloat");
        assert.ok(h.geometryWrites.length > writesAfterFloat, "sibling reflow writes");
        assert.deepEqual(h.focusWrites.filter((f) => f.includes(WIN_A)), [], "no focus write targets the fullscreen subject");
        assert.deepEqual(h.aboveWrites.filter((w) => w.startsWith(`{${WIN_A}}`)), [], "no keepAbove write targets the fullscreen subject");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "no native state write targets the fullscreen subject on unfloat");
        // Retained placement survives the round trip: a third toggle
        // re-floats to the durable frame, never the fullscreen frame.
        entry.requestFloat();
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        const filedRefloat = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(
            (filedRefloat["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: null },
            "refloat still carries no rect (core reuses the retained placement)",
        );
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "3", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-3", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        }));
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME, "retained fullscreen still takes no subject write");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "no native state write targets the fullscreen subject after double toggle");
    });

    it("exiting fullscreen while floating reveals the durable retained placement", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestFloat();
        h.runTimers();
        let call = h.lastCall("DescribeWorld");
        let body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        }));
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME);

        // Exit fullscreen: the next accepted reply places the durable
        // retained float frame (centered 60% first-time fallback).
        h.setFullscreen(WIN_A, false);
        h.fire("windowActivated");
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: null,
        }));
        assert.deepEqual(h.liveFrame(WIN_A), FLOAT_A, "exit reveals the durable retained placement");
    });

    it("exiting fullscreen while tiled reveals the tile slot", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        // Float then unfloat behind retained fullscreen (as above).
        entry.requestFloat();
        h.runTimers();
        let call = h.lastCall("DescribeWorld");
        let body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] },
        }));
        entry.requestFloat();
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled"),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
            result: { op: "toggle-float", win: WIN_A, rect: null },
        }));
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME, "tiled admission stays invisible behind fullscreen");

        // Exit fullscreen: the tile slot applies.
        h.setFullscreen(WIN_A, false);
        h.fire("windowActivated");
        h.runTimers();
        call = h.lastCall("DescribeWorld");
        body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "3", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled"),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
            result: null,
        }));
        assert.deepEqual(h.liveFrame(WIN_A), TILE_A, "exit reveals the tile slot");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_HALF);
    });

    it("fullscreen maximized toggle submits for core refusal with zero writes and no B9 arm", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true, maximizeMode: 3 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        // Tiled fullscreen maximized: adapter submits (core refuses via
        // float-refused-maximize); D5 forbids every subject write and B9
        // never arms under fullscreen.
        entry.requestFloat();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(
            (filed["command"] as Record<string, unknown>)["command"],
            { op: "toggle-float", win: WIN_A, rect: null },
        );
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write to the fullscreen maximized subject");
        assert.ok(!h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")), "B9 never arms under fullscreen");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.maximizeMode, 3, "native maximize state untouched");
        assert.equal(spec?.fullScreen, true, "native fullscreen state untouched");
    });
});

describe("world maximize toggle", () => {
    const maximizeResult = (maximized: boolean, clearFullscreen: boolean, wasMaximized: boolean): Record<string, unknown> => ({
        op: "maximize", win: WIN_A, maximized, clearFullscreen, wasMaximized,
    });
    const tiledWindows = (): Array<Record<string, unknown>> => [
        winRow(WIN_A, TILE_A, "controlled"),
        winRow(WIN_B, TILE_B_HALF, "controlled"),
    ];
    const acceptMaximize = (
        h: ReturnType<typeof makeHarness>,
        rev: string,
        windows: Array<Record<string, unknown>>,
        result: Record<string, unknown>,
    ): void => {
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows, result,
        }));
    };

    it("tiled fullscreen maximize clears full first then maximizes with slots retained", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: WIN_A });
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, false));
        // Realistic setter order: exact KWin `fullScreen` clear first,
        // then `setMaximize(true, true)` back-to-back with no applied
        // readback between them.
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, false, "native fullscreen cleared");
        assert.equal(spec?.maximizeMode, 3, "native maximize applied");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_HALF, "sibling allocation unchanged");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write to the cleared subject");
        assert.deepEqual(h.geometryWrites, [], "slots retained, no reflow writes");
        assert.deepEqual(h.focusWrites.filter((f) => f.includes(WIN_A)), [], "no focus write");
        assert.ok(h.logs.some((l) => l.includes("maximize-applied") && l.includes("clear=fullscreen")), "applied clear log");
    });

    it("second maximize restores the plain original tile", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: WIN_A });
        acceptMaximize(h, "2", tiledWindows(), maximizeResult(false, false, true));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
            `{${WIN_A}}@setMaximize:false,false`,
        ], "restore uses setMaximize only, never a fullscreen write");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, false);
        assert.equal(spec?.maximizeMode, 0, "plain original tile restored");
        assert.deepEqual(h.liveFrame(WIN_A), FULLSCREEN_FRAME, "native frame untouched by the adapter");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_HALF);
    });

    it("floating fullscreen stays floated under maximize and restores the float", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestFloat();
        h.runTimers();
        acceptMaximize(h, "1", [
            winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
            winRow(WIN_B, TILE_B_FULL, "controlled"),
        ], { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "float beneath fullscreen writes nothing");

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: WIN_A });
        acceptMaximize(h, "2", [
            winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
            winRow(WIN_B, TILE_B_FULL, "controlled"),
        ], maximizeResult(true, true, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "float retained, no subject geometry write");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_FULL, "sibling stays full");

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "3", [
            winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
            winRow(WIN_B, TILE_B_FULL, "controlled"),
        ], maximizeResult(false, false, true));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
            `{${WIN_A}}@setMaximize:false,false`,
        ]);
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.maximizeMode, 0, "maximize restored");
        assert.equal(spec?.fullScreen, false);
    });

    it("both flags clears full and retains max", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true, maximizeMode: 3 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: WIN_A });
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, true));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ], "same-requested-mode maximize is a host no-op, full still clears");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, false);
        assert.equal(spec?.maximizeMode, 3, "max retained");
    });

    it("ordinary maximize uses setMaximize only and commandless reports write no fullscreen state", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: WIN_A });
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(
            h.subjectNativeWrites(`{${WIN_A}}`),
            [`{${WIN_A}}@setMaximize:true,true`],
            "ordinary maximize never touches a fullscreen property",
        );
        assert.equal(h.live.get(`{${WIN_A}}`)?.maximizeMode, 3);

        const g = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        g.discover();
        g.fire("windowActivated");
        g.runTimers();
        const call = g.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(g.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: tiledWindows(),
            result: null,
        }));
        assert.deepEqual(g.subjectNativeWrites(`{${WIN_A}}`), [], "no explicit command means zero fullscreen native writes");
        assert.equal(g.live.get(`{${WIN_A}}`)?.fullScreen, true, "fullscreen untouched");
    });

    it("stale preimage, raced focus, and duplicate takes never replay the setters", () => {
        const stale = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        stale.discover();
        stale.nonNullHandle.requestMaximize();
        stale.runTimers();
        acceptMaximize(stale, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(stale.subjectNativeWrites(`{${WIN_A}}`), [], "preimage mismatch writes nothing");
        assert.ok(stale.logs.some((l) => l.includes("reason=source-raced")), "source race logged");

        const raced = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        raced.discover();
        raced.nonNullHandle.requestMaximize();
        raced.runTimers();
        raced.setActive(WIN_B);
        acceptMaximize(raced, "1", tiledWindows(), maximizeResult(true, true, false));
        assert.deepEqual(raced.subjectNativeWrites(`{${WIN_A}}`), [], "focus race writes nothing");
        assert.ok(raced.logs.some((l) => l.includes("reason=focus-raced")), "focus race logged");

        const dup = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        dup.discover();
        dup.nonNullHandle.requestMaximize();
        dup.runTimers();
        acceptMaximize(dup, "1", tiledWindows(), maximizeResult(true, true, false));
        assert.equal(dup.subjectNativeWrites(`{${WIN_A}}`).length, 2);
        (globalThis as Record<string, unknown>)["__takeResult"] = maximizeResult(true, true, false);
        dup.fire("windowActivated");
        dup.runTimers();
        const call = dup.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(dup.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2", {
            windows: tiledWindows(),
            result: null,
        }));
        // The commandless flight never takes, so the duplicated terminal
        // never replays the setters; drop the staged duplicate so later
        // flights cannot consume it.
        delete (globalThis as Record<string, unknown>)["__takeResult"];
        assert.equal(dup.subjectNativeWrites(`{${WIN_A}}`).length, 2, "duplicate never replays");
    });

    it("applied lag still orders clear before maximize and reports unconfirmed", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        // Freeze applied flags: setters record the requested writes but
        // script getters keep returning the pre-write applied values
        // until the Wayland client acks.
        h.stickApplied(WIN_A);
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ], "no applied readback between the setters, clear stays first");
        assert.ok(h.logs.some((l) => l.includes("maximize-unconfirmed")), "pending ack logs unconfirmed, never retried");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, true, "applied fullscreen still pending");
        assert.equal(spec?.maximizeMode, 0, "applied maximize still pending");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write while pending");
    });
});
