// Outer-gap regression: real TS producer payload with host boundaries.
// 800x600 work area, inner 8, outer 12, two tiled windows. The producer
// payload must carry raw bounds plus gaps; the Rust reply must inset all
// four sides; TS apply must write the inset edges. Host shape mirrors the
// live KWin boundary the previous suite missed: QML array-likes (never JS
// arrays), renewed wrappers per enumeration, braced UUID live ids, and
// generic Qt QSize default hints.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { applyWorldReply } from "../src/world-apply";
import type { WorldObservation } from "../src/world-observe";
import { WorldAdapterState } from "../src/world-state";
import { createWorldApplyHost } from "../src/world-native";

const FIXTURE_DIR = join("..", "test-fixtures", "world-wire");
const UPDATE = process.env["UPDATE_WORLD_FIXTURES"] === "1";

const WS = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const OUT = "out-1";

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

function makeSurface() {
    const ws = { id: WS };
    const wins: Array<Record<string, unknown>> = [
        {
            internalId: `{${WIN_A}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 400, height: 600 },
            output: { name: OUT },
            desktops: qmlLike([{ id: WS }]),
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: { width: 0, height: 0 },
            maxSize: { width: 16777215, height: 16777215 },
        },
        {
            internalId: `{${WIN_B}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 400, y: 0, width: 400, height: 600 },
            output: { name: OUT },
            desktops: qmlLike([{ id: WS }]),
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: { width: 0, height: 0 },
            maxSize: { width: 16777215, height: 16777215 },
        },
    ];
    const screens = [{ name: OUT, geometry: { x: 0, y: 0, width: 800, height: 600 } }];
    const surface = {
        windowList: () => qmlLike(wins.map((w) => ({ ...w }))),
        get screens() {
            return qmlLike(screens.map((s) => ({ ...s })));
        },
        get desktops() {
            return qmlLike([{ id: WS }]);
        },
        activeScreen: { name: OUT },
        get activeWindow() {
            return wins[0];
        },
        set activeWindow(_v: unknown) {},
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, wins, ws };
}

function startFlow(surface: object) {
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
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
        readInnerGapFn: () => 8,
        readOuterGapFn: () => 12,
        oracleCallDbus: () => undefined,
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, logs };
}

describe("world outer-gap regression", () => {
    it("real producer carries raw bounds+gaps; reply insets all edges; apply writes them", () => {
        const { surface } = makeSurface();
        const flow = startFlow(surface);
        const disc = [...flow.calls].reverse().find((c) => c.method === "DiscoverWorld");
        assert.ok(disc !== undefined);
        disc.callback(
            JSON.stringify({
                v: 2, adapter: "broker-1", gen: "gen-1", req: "disc-1",
                obs: "0", rev: "0", ready: false, consumed: false,
                gestureSettled: null,
                operationSettled: [], nativeCreations: [],
                warpCursor: null,
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                scopes: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
                workspaces: [],
            }),
        );
        const reportCall = [...flow.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(reportCall !== undefined);
        const payload = (reportCall as { payload: string }).payload;
        // The per-capture source correlation is a fresh nonce per dispatch:
        // mask it for the byte gate (everything else must match exactly).
        const masked = JSON.stringify({ ...JSON.parse(payload), source: "src-masked" });
        if (UPDATE) {
            mkdirSync(FIXTURE_DIR, { recursive: true });
            writeFileSync(join(FIXTURE_DIR, "report-outer-gap.json"), masked);
        } else {
            const expected = readFileSync(join(FIXTURE_DIR, "report-outer-gap.json"), "utf8");
            assert.equal(masked, expected, "producer payload byte gate");
        }
        const body = JSON.parse(payload) as Record<string, unknown>;
        const inner = body["report"] as Record<string, unknown>;
        const screens = inner["screens"] as Array<Record<string, unknown>>;
        assert.equal(screens.length, 1);
        assert.deepEqual(screens[0]?.["full"], [0, 0, 800, 600]);
        assert.equal(screens[0]?.["gap"], 8);
        assert.equal(screens[0]?.["outer"], 12);
        // Generic Qt defaults ride verbatim (host-boundary fidelity).
        assert.ok(payload.includes('"minSize":{"width":0,"height":0},"maxSize":{"width":16777215,"height":16777215}'));
        flow.handle.stop();
    });

    it("shared Rust golden applies inset edges with canonical inner gap", () => {
        let golden: string;
        try {
            golden = readFileSync(join(FIXTURE_DIR, "reply-outer-gap.json"), "utf8");
        } catch (error) {
            void error;
            assert.fail("reply-outer-gap.json missing: generate Rust golden from report-outer-gap.json first");
            return;
        }
        const parsed = parseWorldReply(golden);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            return;
        }
        // Semantic edge assertions (never a bare golden compare): inset
        // 800x600 by 12 gives 776x576 at (12,12); two leaves split 776
        // with one inner gap of 8.
        const frames = new Map(
            parsed.windows.map((w) => [w.win, w.frame] as const),
        );
        const a = frames.get(WIN_A);
        const b = frames.get(WIN_B);
        assert.ok(a !== undefined && b !== undefined);
        const sorted = [a as [number, number, number, number], b as [number, number, number, number]].sort(
            (x, y) => x[0] - y[0],
        );
        assert.deepEqual(sorted[0], [12, 12, 384, 576]);
        assert.deepEqual(sorted[1], [404, 12, 384, 576]);
        assert.equal(sorted[1][0] - (sorted[0][0] + sorted[0][2]), 8);
        // Apply through the QML-like host: geometry writes hit the inset edges.
        // Live wrappers carry braced ids and Qt-default frames that differ
        // from the desired inset frames, so the frame path runs; observation
        // refs are live-wrapper shaped with bare ids, exactly like the
        // qml-list host boundary.
        const liveWins = [
            {
                internalId: `{${WIN_A}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 0, y: 0, width: 400, height: 600 },
                output: { name: OUT },
                desktops: qmlLike([{ id: WS }]),
                minimized: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            },
            {
                internalId: `{${WIN_B}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 400, y: 0, width: 400, height: 600 },
                output: { name: OUT },
                desktops: qmlLike([{ id: WS }]),
                minimized: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            },
        ];
        const applySurface = {
            windowList: () => qmlLike(liveWins.map((w) => ({ ...w }))),
            get screens() {
                return qmlLike([{ name: OUT }]);
            },
            get desktops() {
                return qmlLike([{ id: WS }]);
            },
            currentDesktopForScreen: () => ({ id: WS }),
        };
        const readProp = (value: object, property: string): unknown => {
            try {
                return Reflect.get(value as Record<string, unknown>, property);
            } catch (error) {
                void error;
                return undefined;
            }
        };
        const winEntry = (
            win: string, rect: { x: number; y: number; w: number; h: number },
        ) => ({
            win,
            ref: {
                internalId: win, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: rect.x, y: rect.y, width: rect.w, height: rect.h },
                output: { name: OUT },
                desktops: qmlLike([{ id: WS }]),
                minimized: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            },
            out: OUT, ws: WS,
            rect: { ...rect },
            floating: false, fitExcluded: false, fullscreen: false,
            maximized: false, sticky: false, fixedAuto: false,
            fixedSuppress: false,
            nativeWs: null,
            hints: { minW: null, minH: null, maxW: null, maxH: null },
            minimized: false,
        });
        const observation: WorldObservation = {
            mode: "per-output-local" as const,
            topology: [{
                out: OUT, order: [WS], current: WS, complete: true,
                rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true,
            }],
            bindings: [{ logical: WS, native: WS, out: OUT, pin: false, disp: false, managed: true }],
            domains: [{
                id: OUT, ws: WS,
                bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 12,
                focused: WIN_A, readable: true, hold: "", adjacent: {},
                windows: [
                    winEntry(WIN_A, { x: 0, y: 0, w: 400, h: 600 }),
                    winEntry(WIN_B, { x: 400, y: 0, w: 400, h: 600 }),
                ],
                closed: [], complete: true,
            }],
            minimized: [],
            predicate: "both-axes-fixed" as const,
            complete: true, gesture: false, focusedGlobal: WIN_A,
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        };
        const state = new WorldAdapterState("test-adapter", "test-adapter");
        state.onDiscovery("gen-1", "broker-1");
        (state as unknown as { currentInvalid: string }).currentInvalid = parsed.obs;
        const writes: string[] = [];
        const host = createWorldApplyHost(applySurface as unknown as object, readProp);
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply: parsed,
            log: () => {},
            ...host,
            setGeometry: (_ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                writes.push(`${rect.x},${rect.y},${rect.w},${rect.h}`);
                return true;
            },
        });
        assert.equal(result.stoppedMidApply, false);
        assert.ok(writes.includes("12,12,384,576"), `inset left edge written, got ${JSON.stringify(writes)}`);
        assert.ok(writes.includes("404,12,384,576"), `inset right edge written, got ${JSON.stringify(writes)}`);
    });
});
