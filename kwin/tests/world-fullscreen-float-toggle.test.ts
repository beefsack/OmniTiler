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
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

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
    // Queue/take lane generation: the fake broker binds its replies to
    // this generation (default gen-1); generation-mismatch tests advance
    // it so a rebound press files on the new generation.
    let queueGen = "gen-1";
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
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: queueGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: queueGen }));
                }
            } else if (method === "ReadNativePreimage") {
                // Synchronous empty-seed ack (v1 exact keys): the store
                // hydrates for the live generation with zero retained rows.
                const seedBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({
                    v: 1, correlation_id: seedBody["correlation_id"],
                    outcome: "ok", records: [], stored: 0, returned: 0,
                }));
            } else if (method === "WriteNativePreimage") {
                // Synchronous stored ack (v1 exact keys): the sent snapshot
                // landed, so the count echoes the request record length.
                const writeBody = JSON.parse(payload) as Record<string, unknown>;
                const rows = writeBody["records"];
                const stored = Array.isArray(rows) ? rows.length : 0;
                callback(JSON.stringify({
                    v: 1, correlation_id: writeBody["correlation_id"],
                    outcome: "stored", stored,
                }));
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
        // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
        // output): the producer adapter lifetime, the serving generation,
        // the capture source correlation and native watermark. This
        // scripted harness advances broker obs and revision together.
        // Terminal results ride the take lane requester-only, never the
        // capture reply.
        const { result, ...snapshot } = extra;
        if (result !== undefined) {
            (globalThis as Record<string, unknown>)["__takeResult"] = result;
        }
        const text = JSON.stringify({
            v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
            ready: true, consumed: false,
            source,
            nativeInvalid,
            clock: "1",
            gestureSettled: null, gestureAssoc: null,
            closeFocus: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [{ scope: { kind: "workspace", out: OUT, ws: WS_A }, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
            ...snapshot,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        return text;
    };
    const discover = (gen = "gen-1"): void => {
        // Genuine v4 connection bootstrap: the handshake reply echoes the
        // producer adapter lifetime and mints the serving generation; the
        // revisioned settings snapshot must read accepted before the first
        // report flies.
        const handshake = [...calls].reverse().find((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
            } catch {
                return false;
            }
        });
        assert.ok(handshake !== undefined, "expected DiscoverWorld/handshake");
        const handshakeBody = JSON.parse(handshake.payload) as Record<string, unknown>;
        handshake.callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter: "test-adapter", req: handshakeBody["req"],
            ready: false, kind: "", gen,
        }));
        const settings = [...calls].reverse().find((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
            } catch {
                return false;
            }
        });
        assert.ok(settings !== undefined, "expected DiscoverWorld/settings");
        const settingsBody = JSON.parse(settings.payload) as Record<string, unknown>;
        settings.callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter", gen,
            req: settingsBody["req"], rev: settingsBody["rev"], status: "accepted", diagnostics: [],
        }));
        runTimers();
        // Settle the discovery-triggered observation report first: the
        // entry auto-dispatches one observation-only DescribeWorld on bind.
        // Replying here leaves the flight idle for the test body.
        const auto = lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        assert.equal(autoBody["command"], undefined, "captures never carry an inline command");
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
        setQueueGen: (gen: string): void => {
            queueGen = gen;
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
    cmdSeq: null,
    visibility: null, visibility_auth: "untouched", visibility_hold: "",
    state: [],
    ...extra,
});

describe("world fullscreen float toggle", () => {
    // Ordinary keyboard windowed intents (requestFloat/requestMaximize/
    // requestFullscreen below) file win:{currentFocus:true}, resolved at
    // core execution time (docs/decisions/commands.md#kde-rapid-input).
    // Terminal take results and reply rows keep explicit subject ids.
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
            filed["command"] as Record<string, unknown>,
            { op: "toggle-float", win: { currentFocus: true }, rect: null },
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
            filedUnfloat["command"] as Record<string, unknown>,
            { op: "toggle-float", win: { currentFocus: true }, rect: null },
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
            filedRefloat["command"] as Record<string, unknown>,
            { op: "toggle-float", win: { currentFocus: true }, rect: null },
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
            filed["command"] as Record<string, unknown>,
            { op: "toggle-float", win: { currentFocus: true }, rect: null },
        );
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write to the fullscreen maximized subject");
        assert.ok(!h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")), "B9 never arms under fullscreen");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.maximizeMode, 3, "native maximize state untouched");
        assert.equal(spec?.fullScreen, true, "native fullscreen state untouched");
    });
});

describe("world maximize toggle", () => {
    // Filed requestMaximize commands carry the implicit execution-time
    // marker (docs/decisions/commands.md#kde-rapid-input); descriptive
    // take results below keep explicit subject ids.
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
    // Controlled state rows are the only native authority (atomic
    // migration): descriptive take results never actuate. Each lane is an
    // exact v4 state target (property/value/auth/reason/cmdSeq); paired
    // clear-first legs share one cmdSeq across fullscreen:false and
    // maximized so the executor clears before maximizing.
    const maxRow = (value: boolean, cmdSeq: string | null): Record<string, unknown> => ({
        property: "maximized", value, auth: "controlled", reason: "", cmdSeq,
    });
    const fullRow = (value: boolean, cmdSeq: string | null): Record<string, unknown> => ({
        property: "fullscreen", value, auth: "controlled", reason: "", cmdSeq,
    });
    const clearMaxWindows = (cmdSeq: string): Array<Record<string, unknown>> => [
        winRow(WIN_A, TILE_A, "controlled", { state: [fullRow(false, cmdSeq), maxRow(true, cmdSeq)] }),
        winRow(WIN_B, TILE_B_HALF, "controlled"),
    ];
    const maxOnlyWindows = (value: boolean, cmdSeq: string): Array<Record<string, unknown>> => [
        winRow(WIN_A, TILE_A, "controlled", { state: [maxRow(value, cmdSeq)] }),
        winRow(WIN_B, TILE_B_HALF, "controlled"),
    ];
    const nextAccept = (
        h: ReturnType<typeof makeHarness>,
        rev: string,
        extra: Record<string, unknown>,
    ): void => {
        h.fire("windowActivated");
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, rev, extra));
    };
    // Direct accept for the already-in-flight flight (right after
    // request+runTimers): firing first would invalidate the flight
    // watermark and supersede it (no setters run), so a fresh flight is
    // only dispatched after an accept consumes the previous one.
    const directAccept = (
        h: ReturnType<typeof makeHarness>,
        rev: string,
        extra: Record<string, unknown>,
    ): void => {
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, rev, extra));
    };
    const legacyPendingTokens = ["maximize-pending", "maximize-applied", "maximize-refused", "fullscreen-pending", "fullscreen-applied", "fullscreen-refused", "native-pending-superseded"];

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
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        // Descriptive take result alone never actuates (retired legacy
        // pending path): zero native writes from the terminal alone.
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        // The matching Controlled rows drive the executor: exact KWin
        // `fullScreen` clear first, then `setMaximize(true, true)` with no
        // applied readback between them.
        nextAccept(h, "2", { windows: clearMaxWindows("1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);
        assert.ok(h.logs.some((l) => l.includes("native-state-pending") && l.includes("target=fullscreen")), "clear executed");
        assert.ok(h.logs.some((l) => l.includes("native-state-pending") && l.includes("target=maximized")), "maximize executed");
        assert.ok(!legacyPendingTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, false, "native fullscreen cleared");
        assert.equal(spec?.maximizeMode, 3, "native maximize applied");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_HALF, "sibling allocation unchanged");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write to the cleared subject");
        assert.deepEqual(h.geometryWrites, [], "slots retained, no reflow writes");
        assert.deepEqual(h.focusWrites.filter((f) => f.includes(WIN_A)), [], "no focus write");
        // A later report without the rows retries nothing: the single
        // attempt stands, confirmation is the write itself.
        nextAccept(h, "3", { windows: tiledWindows() });
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "rowless report reissues nothing");
        assert.ok(!legacyPendingTokens.some((t) => h.logs.some((l) => l.includes(t))), "still no legacy pending traffic");
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
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAccept(h, "2", { windows: clearMaxWindows("1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);

        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        acceptMaximize(h, "3", tiledWindows(), maximizeResult(false, false, true));
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "descriptive restore result writes nothing");
        // Restore is a maximize-only Controlled row: setMaximize without
        // any fullscreen write.
        nextAccept(h, "4", { windows: maxOnlyWindows(false, "2") });
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
        // A later rowless report reissues nothing and never fails the
        // shortcut label (the executor owns no labels).
        nextAccept(h, "5", { windows: tiledWindows() });
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 3, "no replay");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "superseded first attempt never fails");
        assert.ok(!legacyPendingTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
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
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        acceptMaximize(h, "2", [
            winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
            winRow(WIN_B, TILE_B_FULL, "controlled"),
        ], maximizeResult(true, true, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAccept(h, "3", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true, state: [fullRow(false, "1"), maxRow(true, "1")] }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
        });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ]);
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "float retained, no subject geometry write");
        assert.deepEqual(h.liveFrame(WIN_B), TILE_B_FULL, "sibling stays full");

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "4", [
            winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
            winRow(WIN_B, TILE_B_FULL, "controlled"),
        ], maximizeResult(false, false, true));
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "descriptive restore result writes nothing");
        nextAccept(h, "5", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true, state: [maxRow(false, "2")] }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
        });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
            `{${WIN_A}}@setMaximize:false,false`,
        ]);
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.maximizeMode, 0, "maximize restored");
        assert.equal(spec?.fullScreen, false);
        // A later rowless report reissues nothing and never fails labels.
        nextAccept(h, "6", {
            windows: [
                winRow(WIN_A, FLOAT_A, "untouched", { frame_op: "op-1", clear_op: "", floating: true }),
                winRow(WIN_B, TILE_B_FULL, "controlled"),
            ],
        });
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 3, "no replay of the restore");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure");
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
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, true, true));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAccept(h, "2", { windows: clearMaxWindows("1") });
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
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAccept(h, "2", { windows: maxOnlyWindows(true, "1") });
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

    it("stale replies drop, focus never gates, and duplicate rows never replay the setters", () => {
        // Stale-req replies settle nothing: the later fresh Controlled
        // rows still execute exactly once.
        const stale = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        stale.discover();
        stale.nonNullHandle.requestMaximize();
        stale.runTimers();
        {
            stale.fire("windowActivated");
            stale.runTimers();
            const flight = stale.lastCall("DescribeWorld");
            const flightBody = JSON.parse(flight.payload) as Record<string, unknown>;
            const staleReply = stale.accepted("gen-1", flightBody["source"] as string, flightBody["nativeInvalid"] as string, "9", {
                windows: clearMaxWindows("1"),
            });
            flight.callback(JSON.stringify({ ...(JSON.parse(staleReply) as Record<string, unknown>), req: "req-999", source: "req-999" }));
        }
        assert.deepEqual(stale.subjectNativeWrites(`{${WIN_A}}`), [], "stale reply writes nothing");
        assert.ok(stale.logs.some((l) => l.includes("stale reason=stale-req")), "wrong-req reply goes stale");
        stale.fire("windowActivated");
        stale.runTimers();
        {
            const call = stale.lastCall("DescribeWorld");
            const body = JSON.parse(call.payload) as Record<string, unknown>;
            call.callback(stale.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
                windows: clearMaxWindows("1"),
            }));
        }
        assert.deepEqual(stale.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ], "fresh rows execute after the stale drop");

        // Core owns the verdict: a moved focus never gates Controlled
        // state rows (no permanent focus gate for state rows).
        const raced = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        raced.discover();
        raced.nonNullHandle.requestMaximize();
        raced.runTimers();
        raced.setActive(WIN_B);
        // The request flight is still in flight: accept it directly (a
        // fire here would supersede it before the rows land).
        directAccept(raced, "1", { windows: clearMaxWindows("1") });
        assert.deepEqual(raced.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ], "focus elsewhere still executes");
        assert.ok(!raced.logs.some((l) => l.includes("focus-raced")), "no focus fence for state rows");

        // Repeating the identical target+cmdSeq rows never replays: the
        // latest identity stands until authority changes.
        const dup = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        dup.discover();
        dup.nonNullHandle.requestMaximize();
        dup.runTimers();
        // The request flight is still in flight: accept it directly (a
        // fire here would supersede it before the rows land).
        directAccept(dup, "1", { windows: clearMaxWindows("1") });
        assert.equal(dup.subjectNativeWrites(`{${WIN_A}}`).length, 2);
        nextAccept(dup, "2", { windows: clearMaxWindows("1") });
        assert.equal(dup.subjectNativeWrites(`{${WIN_A}}`).length, 2, "duplicate rows never replay");
        assert.ok(!dup.logs.some((l) => l.includes("shortcut-failed")), "duplicates never fail labels");
    });

    it("applied lag records ordered writes at accept with no replay on later reports", () => {
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
        // until the Wayland client acks. Execution still runs sync at
        // accept (persist-before-write, no applied readback between the
        // setters); only the fake's readback lags.
        h.stickApplied(WIN_A);
        // The request flight is still in flight: accept it directly (a
        // fire here would supersede it before the rows land).
        directAccept(h, "1", { windows: clearMaxWindows("1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=false`,
            `{${WIN_A}}@setMaximize:true,true`,
        ], "no applied readback between the setters, clear stays first");
        assert.ok(h.logs.some((l) => l.includes("native-state-pending") && l.includes("target=fullscreen")), "clear executed at accept");
        assert.ok(h.logs.some((l) => l.includes("native-state-pending") && l.includes("target=maximized")), "maximize executed at accept");
        assert.ok(!legacyPendingTokens.some((t) => h.logs.some((l) => l.includes(t))), "no legacy pending traffic");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "execution never fails the shortcut");
        const spec = h.live.get(`{${WIN_A}}`);
        assert.equal(spec?.fullScreen, true, "applied fullscreen still pending");
        assert.equal(spec?.maximizeMode, 0, "applied maximize still pending");
        assert.deepEqual(h.subjectWrites(`{${WIN_A}}`), [], "no geometry write");

        // Client ack lands before the next report: the rowless report
        // reads the fresh facts and reissues nothing.
        h.setFullscreen(WIN_A, false);
        h.setMaximized(WIN_A, 3);
        nextAccept(h, "2", { windows: tiledWindows() });
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "settlement never fails");
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "report reads, never re-issues");
    });

    it("stuck applied records one attempt with no retry and no label failure", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: FULLSCREEN_FRAME[0], y: FULLSCREEN_FRAME[1], width: FULLSCREEN_FRAME[2], height: FULLSCREEN_FRAME[3] }, { fullScreen: true }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        h.stickApplied(WIN_A);
        // The request flight is still in flight: accept it directly (a
        // fire here would supersede it before the rows land).
        directAccept(h, "1", { windows: clearMaxWindows("1") });
        assert.ok(h.logs.some((l) => l.includes("native-state-pending")), "execution recorded");
        // The host never acts: applied flags stay put, but the single
        // attempt still stands exactly once with no retry and no label
        // failure (the executor owns no shortcut labels).
        nextAccept(h, "2", { windows: tiledWindows() });
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure without authority");
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "one attempt only, never retried");
    });

    it("maximize and fullscreen file as independent commands with no shared pending", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        // A newer fullscreen input files its own existing command with
        // zero direct writes: there is no shared pending lane to
        // supersede, and the older maximize rows still execute on their
        // own authority.
        entry.requestFullscreen();
        h.runTimers();
        const fsFiled = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(fsFiled["command"], { op: "toggle-fullscreen", win: { currentFocus: true } }, "fullscreen files the existing core command");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "enqueue writes nothing directly");
        assert.ok(!h.logs.some((l) => l.includes("native-pending-superseded")), "no shared pending to supersede");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "independent commands never fail each other");
        // The fullscreen request flight is still in flight: accept it
        // directly (a fire here would supersede it before the rows land).
        directAccept(h, "2", { windows: maxOnlyWindows(true, "1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "maximize rows execute");
        nextAccept(h, "3", {
            windows: [
                winRow(WIN_A, TILE_A, "controlled", { state: [fullRow(true, "2")] }),
                winRow(WIN_B, TILE_B_HALF, "controlled"),
            ],
        });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@setMaximize:true,true`,
            `{${WIN_A}}@fullScreen=true`,
        ], "fullscreen rows execute independently");
        assert.ok(!legacyPendingTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("focus elsewhere still executes the Controlled maximize rows", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        // Core owns the verdict: moving focus before the rows land never
        // gates them (no permanent focus gate for state rows).
        h.setActive(WIN_B);
        nextAccept(h, "2", { windows: maxOnlyWindows(true, "1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "rows execute despite focus elsewhere");
        assert.ok(!h.logs.some((l) => l.includes("focus-raced")), "no focus fence for state rows");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure");
    });

    it("departed lifetime at the boundary drops the rows with zero writes", () => {
        const h = makeHarness([
            termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
            termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
        ]);
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximize(h, "1", tiledWindows(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        h.live.delete(`{${WIN_A}}`);
        nextAccept(h, "2", { windows: maxOnlyWindows(true, "1") });
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "departed window takes no writes");
        assert.ok(h.logs.some((l) => l.includes("native-state-dropped") && l.includes("observe-missing")), "departure drops the lane");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure");
    });
});

describe("world native pending settlement", () => {
    // Filed requestFullscreen/requestMaximize commands carry the implicit
    // execution-time marker (docs/decisions/commands.md#kde-rapid-input);
    // the gen-2 leg below then verifies the rebound generation counts.
    const maximizeResult = (maximized: boolean, clearFullscreen: boolean, wasMaximized: boolean): Record<string, unknown> => ({
        op: "maximize", win: WIN_A, maximized, clearFullscreen, wasMaximized,
    });
    const tiledOrdinary = (): Array<Record<string, unknown>> => [
        winRow(WIN_A, TILE_A, "controlled"),
        winRow(WIN_B, TILE_B_HALF, "controlled"),
    ];
    const acceptMaximizeGen = (
        h: ReturnType<typeof makeHarness>,
        gen: string,
        rev: string,
        windows: Array<Record<string, unknown>>,
        result: Record<string, unknown> | null,
    ): void => {
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted(gen, body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows, result,
        }));
    };
    const tiledPair = (): LiveSpec[] => [
        termSpec(WIN_A, { x: 0, y: 0, width: 400, height: 600 }),
        termSpec(WIN_B, { x: 400, y: 0, width: 400, height: 600 }),
    ];
    // Controlled state rows are the only native authority (atomic
    // migration): descriptive take results never actuate. Each lane is an
    // exact v4 state target (property/value/auth/reason/cmdSeq).
    const maxRow = (value: boolean, cmdSeq: string | null): Record<string, unknown> => ({
        property: "maximized", value, auth: "controlled", reason: "", cmdSeq,
    });
    const fullRow = (value: boolean, cmdSeq: string | null): Record<string, unknown> => ({
        property: "fullscreen", value, auth: "controlled", reason: "", cmdSeq,
    });
    const maxOnlyOrdinary = (value: boolean, cmdSeq: string): Array<Record<string, unknown>> => [
        winRow(WIN_A, TILE_A, "controlled", { state: [maxRow(value, cmdSeq)] }),
        winRow(WIN_B, TILE_B_HALF, "controlled"),
    ];
    const nextAcceptGen = (
        h: ReturnType<typeof makeHarness>,
        gen: string,
        rev: string,
        windows: Array<Record<string, unknown>>,
    ): void => {
        h.fire("windowActivated");
        h.runTimers();
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted(gen, body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows,
        }));
    };
    // Direct accept for the already-in-flight flight (right after
    // request+runTimers): firing first would invalidate the flight
    // watermark and supersede it (no setters run), so a fresh flight is
    // only dispatched after an accept consumes the previous one.
    const directAcceptGen = (
        h: ReturnType<typeof makeHarness>,
        gen: string,
        rev: string,
        windows: Array<Record<string, unknown>>,
    ): void => {
        const call = h.lastCall("DescribeWorld");
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        call.callback(h.accepted(gen, body["source"] as string, body["nativeInvalid"] as string, rev, {
            windows,
        }));
    };
    const legacyTokens = ["maximize-pending", "maximize-applied", "maximize-refused", "fullscreen-pending", "fullscreen-applied", "fullscreen-refused", "native-pending-superseded"];

    it("ordinary maximize executes sync at accept, then restore executes", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        // Freeze applied flags so issuance records the requested writes
        // while the script getters keep returning the pre-write values
        // until the client acks (Wayland applied lag).
        h.stickApplied(WIN_A);
        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        assert.equal(h.live.get(`{${WIN_A}}`)?.maximizeMode, 0, "applied maximize still pre-write");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no refusal before the rows land");

        // The Controlled rows execute sync at accept (persist-before-write);
        // the stuck readback lags but records the same single attempt.
        nextAcceptGen(h, "gen-1", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`]);
        assert.ok(h.logs.some((l) => l.includes("native-state-pending") && l.includes("target=maximized")), "execution recorded");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "execution never fails the label");

        // Client ack lands before the next report: the rowless report
        // reads the fresh facts and reissues nothing.
        h.setMaximized(WIN_A, 3);
        nextAcceptGen(h, "gen-1", "3", tiledOrdinary());
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "report reads, never re-issues");

        // The restore rows run the same single-attempt path back.
        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "4", tiledOrdinary(), maximizeResult(false, false, true));
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "descriptive restore result writes nothing");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no refusal before the restore rows");
        nextAcceptGen(h, "gen-1", "5", maxOnlyOrdinary(false, "2"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@setMaximize:true,true`,
            `{${WIN_A}}@setMaximize:false,false`,
        ]);
        h.setMaximized(WIN_A, 0);
        nextAcceptGen(h, "gen-1", "6", tiledOrdinary());
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 2, "one attempt each, never retried");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("fullscreen enter and exit file commands with zero direct writes, rows execute", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        // Freeze applied flags so the executor records the requested
        // write while the script getter keeps returning the pre-write
        // value until the client acks.
        h.stickApplied(WIN_A);
        entry.requestFullscreen();
        h.runTimers();
        const enterFiled = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(enterFiled["command"], { op: "toggle-fullscreen", win: { currentFocus: true } }, "enter files the existing core command");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "no legacy pending traffic");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no refusal before the rows land");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "enqueue writes nothing directly");
        assert.equal(h.live.get(`{${WIN_A}}`)?.fullScreen, false, "applied fullscreen still pre-write");
        // The Controlled rows execute sync at the accept. The request
        // flight is still in flight: accept it directly (a fire here
        // would supersede it before the rows land).
        directAcceptGen(h, "gen-1", "1", [
            winRow(WIN_A, TILE_A, "controlled", { state: [fullRow(true, "1")] }),
            winRow(WIN_B, TILE_B_HALF, "controlled"),
        ]);
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@fullScreen=true`], "enter rows execute");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "execution never fails the label");
        // Client ack lands before the next report: the rowless report
        // reads the fresh facts and reissues nothing.
        h.setFullscreen(WIN_A, true);
        nextAcceptGen(h, "gen-1", "2", tiledOrdinary());
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@fullScreen=true`], "no setter replay");

        entry.requestFullscreen();
        h.runTimers();
        const exitFiled = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(exitFiled["command"], { op: "toggle-fullscreen", win: { currentFocus: true } }, "exit files the existing core command");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=true`,
        ], "exit enqueue writes nothing directly");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no refusal before the exit rows");
        // The exit request flight is still in flight: accept it directly.
        directAcceptGen(h, "gen-1", "3", [
            winRow(WIN_A, TILE_A, "controlled", { state: [fullRow(false, "2")] }),
            winRow(WIN_B, TILE_B_HALF, "controlled"),
        ]);
        h.setFullscreen(WIN_A, false);
        nextAcceptGen(h, "gen-1", "4", tiledOrdinary());
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [
            `{${WIN_A}}@fullScreen=true`,
            `{${WIN_A}}@fullScreen=false`,
        ], "one attempt each, never retried");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("stuck fullscreen records one attempt with no replay and no label failure", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        h.stickApplied(WIN_A);
        entry.requestFullscreen();
        h.runTimers();
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "enqueue writes nothing directly");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "enqueue never fails the shortcut");
        // The request flight is still in flight: accept it directly (a
        // fire here would supersede it before the rows land).
        directAcceptGen(h, "gen-1", "1", [
            winRow(WIN_A, TILE_A, "controlled", { state: [fullRow(true, "1")] }),
            winRow(WIN_B, TILE_B_HALF, "controlled"),
        ]);
        assert.ok(h.logs.some((l) => l.includes("native-state-pending")), "execution recorded");
        // The host never acts: the single attempt stands exactly once
        // with no retry and no label failure.
        nextAcceptGen(h, "gen-1", "2", tiledOrdinary());
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure without host action");
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "one attempt only, never retried");
        nextAcceptGen(h, "gen-1", "3", tiledOrdinary());
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed") && l.includes("fullscreen")), "never fails");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("an intervening queued command files alongside with no shared pending", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        // A queued focus input files its own command: there is no shared
        // pending lane to supersede, and the maximize rows still execute
        // on their own authority.
        entry.requestFocus("left");
        h.runTimers();
        const focusFiled = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.equal((focusFiled["command"] as Record<string, unknown>)["op"], "focus-directional", "focus files its own command");
        assert.ok(!h.logs.some((l) => l.includes("native-pending-superseded")), "no shared pending to supersede");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "independent commands never fail each other");
        // The focus request flight is still in flight: accept it directly
        // (a fire here would supersede it before the rows land).
        directAcceptGen(h, "gen-1", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "maximize rows execute");
        nextAcceptGen(h, "gen-1", "3", tiledOrdinary());
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "no reissue");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("a rejected reply writes nothing and the next fresh rows still execute", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        h.fire("windowActivated");
        h.runTimers();
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "busy", message: "busy", gen: "gen-1",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "rejection writes nothing");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "rejection never fails the shortcut label");
        // The rejected flight settles nothing: the next fresh Controlled
        // rows still execute exactly once.
        nextAcceptGen(h, "gen-1", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "fresh rows execute after rejection");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
        nextAcceptGen(h, "gen-1", "3", tiledOrdinary());
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "never replayed");
    });

    it("a stale-req reply writes nothing and the next fresh rows execute", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        h.fire("windowActivated");
        h.runTimers();
        const flight = h.lastCall("DescribeWorld");
        const body = JSON.parse(flight.payload) as Record<string, unknown>;
        const staleReply = h.accepted("gen-1", body["source"] as string, body["nativeInvalid"] as string, "9", {
            windows: maxOnlyOrdinary(true, "1"),
        });
        flight.callback(JSON.stringify({ ...(JSON.parse(staleReply) as Record<string, unknown>), req: "req-999", source: "req-999" }));
        assert.ok(h.logs.some((l) => l.includes("stale reason=stale-req")), "wrong-req reply goes stale");
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "stale reply writes nothing");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "stale reply fails nothing");
        // The stale drop released the flight and the redispatch above
        // sent a fresh one: accept it directly (a fire here would
        // supersede it before the rows land).
        h.runTimers();
        directAcceptGen(h, "gen-1", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "next fresh rows execute");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "execution never fails");
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "never replayed");
    });

    it("a generation mismatch drops the rows and a fresh press flows on gen-2", () => {
        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;

        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        h.fire("windowActivated");
        h.runTimers();
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        assert.ok(h.logs.some((l) => l.includes("generation-mismatch recovery=rediscover")), "mismatch rediscovers");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "mismatch never fails the shortcut label");
        // The v4 rebind resends the connection pair (handshake then
        // revisioned settings) on the shared DiscoverWorld route: the
        // settings resend waits on the handshake accept, so pre-answer the
        // route carries exactly two handshakes and one settings snapshot.
        const handshakeCalls = h.calls.filter((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse((c as { payload: string }).payload) as Record<string, unknown>)["type"] === "handshake";
            } catch {
                return false;
            }
        });
        const settingsCalls = h.calls.filter((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse((c as { payload: string }).payload) as Record<string, unknown>)["type"] === "settings";
            } catch {
                return false;
            }
        });
        assert.equal(handshakeCalls.length, 2, "fresh handshake issued");
        assert.equal(settingsCalls.length, 1, "settings resend waits on the handshake accept");
        const reHandshake = handshakeCalls.pop() as { payload: string; callback: (reply: unknown) => void };
        const reHandshakeBody = JSON.parse(reHandshake.payload) as Record<string, unknown>;
        reHandshake.callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter: "test-adapter", req: reHandshakeBody["req"],
            ready: false, kind: "", gen: "gen-2",
        }));
        // The handshake accept releases the settings resend: the pair
        // counts four on the shared route.
        assert.equal(h.calls.filter((c) => c.method === "DiscoverWorld").length, 4, "rebind pair complete after handshake accept");
        const reSettings = h.calls.filter((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse((c as { payload: string }).payload) as Record<string, unknown>)["type"] === "settings";
            } catch {
                return false;
            }
        }).pop() as { payload: string; callback: (reply: unknown) => void };
        const reSettingsBody = JSON.parse(reSettings.payload) as Record<string, unknown>;
        reSettings.callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter", gen: "gen-2",
            req: reSettingsBody["req"], rev: reSettingsBody["rev"], status: "accepted", diagnostics: [],
        }));
        {
            const auto = h.lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload) as Record<string, unknown>;
            auto.callback(h.accepted("gen-2", autoBody["source"] as string, autoBody["nativeInvalid"] as string, "0"));
        }
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "dead generation wrote nothing");
        // The rebound generation rebinds the fake broker lanes too.
        h.setQueueGen("gen-2");
        // A fresh press flows on the rebound generation: the take result
        // stays descriptive, the Controlled rows execute sync at accept.
        entry.requestMaximize();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-maximize", win: { currentFocus: true } });
        acceptMaximizeGen(h, "gen-2", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAcceptGen(h, "gen-2", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "fresh press executes on gen-2");
        nextAcceptGen(h, "gen-2", "3", tiledOrdinary());
        assert.equal(h.subjectNativeWrites(`{${WIN_A}}`).length, 1, "no replay");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no label failure through the rotation");
        assert.ok(!legacyTokens.some((t) => h.logs.some((l) => l.includes(t))), "legacy pending path dormant");
    });

    it("stop isolates the entry: a fresh harness flows unaffected", () => {
        const staleH = makeHarness(tiledPair());
        staleH.discover();
        staleH.nonNullHandle.requestMaximize();
        staleH.runTimers();
        acceptMaximizeGen(staleH, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(staleH.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        const callsBefore = staleH.calls.length;
        staleH.nonNullHandle.stop();
        staleH.fire("windowActivated");
        staleH.runTimers();
        assert.equal(staleH.calls.length, callsBefore, "stopped entry files nothing");
        assert.ok(!staleH.logs.some((l) => l.includes("native-state-pending")), "stopped entry executes nothing");

        const h = makeHarness(tiledPair());
        h.discover();
        const entry = h.nonNullHandle;
        entry.requestMaximize();
        h.runTimers();
        acceptMaximizeGen(h, "gen-1", "1", tiledOrdinary(), maximizeResult(true, false, false));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [], "descriptive take result writes nothing");
        nextAcceptGen(h, "gen-1", "2", maxOnlyOrdinary(true, "1"));
        assert.deepEqual(h.subjectNativeWrites(`{${WIN_A}}`), [`{${WIN_A}}@setMaximize:true,true`], "fresh entry executes");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "fresh entry never fails");
    });
});
