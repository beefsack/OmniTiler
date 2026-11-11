// Opaque intent persistence through the production entry (offline only, no
// D-Bus, no KWin): the bounded seed hydrates before the first admissible
// report, the live join carries exact bytes, and only accepted core
// `intentWrites` update persistence. Disabled harnesses bypass with an
// empty map.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function makeSurface() {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        },
        {
            internalId: "win-b", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws2],
            minimized: true, hidden: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        },
    ];
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0],
        currentDesktop: ws1,
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref as typeof world.activeWindow;
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            world.currentDesktop = ref as FakeDesktop;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as FakeDesktop;
        },
        sendClientToOutput: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface Timer {
    delay: number;
    cb: () => void;
    cancelled: boolean;
}

const DATA_A = "{\"v\":1,\"float\":true}";
const DATA_B = "{\"v\":1,\"float\":false}";
const DATA_GHOST = "{\"v\":1,\"tile\":true}";

function startFlow(bus?: {
    stored: Array<{ win: string; data: string }>;
    degradeReads: boolean;
    holdWrites?: boolean;
}, liveSettings?: { gap: number }) {
    const { surface, world } = makeSurface();
    const calls: Call[] = [];
    const timers: Timer[] = [];
    const logs: string[] = [];
    let gap = liveSettings?.gap ?? 8;
    let configHandler: (() => void) | null = null;
    const fakeOptions = {
        configChanged: {
            connect: (h: () => void): void => {
                configHandler = h;
            },
            disconnect: (): void => {
                configHandler = null;
            },
        },
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        floatIntent: true,
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (bus !== undefined && (method === "ReadFloatIntent" || method === "WriteFloatIntent")) {
                calls.push({ method, payload, callback });
                const body = JSON.parse(payload) as Record<string, unknown>;
                const corr = body["correlation_id"] as string;
                if (method === "ReadFloatIntent") {
                    if (bus.degradeReads) {
                        callback(JSON.stringify({
                            v: 2, correlation_id: corr, outcome: "degraded",
                            records: [], stored: 0, returned: 0, reason: "unreadable",
                        }));
                    } else {
                        callback(JSON.stringify({
                            v: 2, correlation_id: corr, outcome: "ok",
                            records: bus.stored, stored: bus.stored.length, returned: bus.stored.length,
                        }));
                    }
                    return;
                }
                bus.stored = body["records"] as Array<{ win: string; data: string }>;
                if (bus.holdWrites === true) {
                    return;
                }
                callback(JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored: bus.stored.length }));
                return;
            }
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const t: Timer = { delay, cb, cancelled: false };
            timers.push(t);
            return () => {
                t.cancelled = true;
            };
        },
        log: (m) => {
            logs.push(m);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readInnerGapFn: () => gap,
        readOuterGapFn: () => 0,
        options: fakeOptions,
    });
    assert.ok(handle !== null);
    const geometryOf = (id: string): unknown => {
        const found = (world.wins as Array<Record<string, unknown>>).find((w) => w["internalId"] === id);
        return (found as Record<string, unknown>)?.["frameGeometry"] ?? null;
    };
    const pump = (maxDelay: number): void => {
        for (;;) {
            const at = timers.findIndex((t) => !t.cancelled && t.delay <= maxDelay);
            if (at < 0) {
                return;
            }
            const next = timers.splice(at, 1)[0] as Timer;
            next.cb();
        }
    };
    const lastCall = (method: string, type?: string): Call => {
        const found = [...calls].reverse().find((c) => {
            if (c.method !== method) {
                return false;
            }
            if (type === undefined) {
                return true;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === type;
            } catch (error) {
                void error;
                return false;
            }
        });
        assert.ok(found !== undefined, `expected ${method}${type === undefined ? "" : `/${type}`}`);
        return found as Call;
    };
    const hasCall = (method: string): boolean => calls.some((c) => c.method === method);
    const setGap = (v: number): void => {
        gap = v;
    };
    const fireConfigChanged = (): void => {
        assert.ok(configHandler !== null, "expected configChanged signal wiring");
        (configHandler as () => void)();
    };
    return { handle: handle as NonNullable<typeof handle>, calls, timers, logs, pump, lastCall, hasCall, geometryOf, setGap, fireConfigChanged };
}

type Flow = ReturnType<typeof startFlow>;

function answerHandshake(flow: Flow, gen = "gen-1"): void {
    const disc = flow.lastCall("DiscoverWorld", "handshake");
    const req = (JSON.parse(disc.payload) as Record<string, unknown>)["req"] as string;
    disc.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(flow: Flow, gen = "gen-1"): void {
    const settings = flow.lastCall("DiscoverWorld", "settings");
    const body = JSON.parse(settings.payload) as Record<string, unknown>;
    flow.lastCall("DiscoverWorld", "settings").callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

// Genuine 18-key v4 window row (retired 13-key shape refuses).
function winRow(win: string, ws: string): Record<string, unknown> {
    return {
        win, out: "out", ws,
        alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "", clear_op: "",
        cmdSeq: null,
        visibility: { shown: true, rev: "1", fallback: false },
        visibility_auth: "controlled", visibility_hold: "",
        state: [],
    };
}

function buildAccept(
    gen: string,
    envelope: Record<string, unknown>,
    intentWrites: ReadonlyArray<Record<string, unknown>>,
    rev = "1",
): string {
    return JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: envelope["source"], obs: "1", rev,
        ready: true, consumed: false,
        scopes: [{ scope: { kind: "output", out: "out" }, auth: "controlled", reason: "" }],
        workspaces: [
            { id: "ws1", out: "out", bind: "", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "ws2", out: "out", bind: "", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
        ],
        outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
        windows: [winRow("win-a", "ws1"), winRow("win-b", "ws2")],
        transfers: [], effects: [], closeFocus: null, result: null,
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        intentWrites,
        presentation: DISABLED_PRESENTATION,
        switchSettled: [], diagnostics: [],
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
    });
}

// Every authored accept is verified with the production parser before the
// callback runs, so a refused shape fails loudly instead of silently
// inflating counts.
function acceptCapture(
    gen: string,
    envelope: Record<string, unknown>,
    intentWrites: ReadonlyArray<Record<string, unknown>>,
    rev = "1",
): string {
    const text = buildAccept(gen, envelope, intentWrites, rev);
    const parsed = parseWorldReply(text);
    assert.ok(isCoreWorldReply(parsed), "authored capture accept binds strict v4");
    return text;
}

function answerWrite(flow: Flow, reply: string): void {
    flow.lastCall("WriteFloatIntent").callback(reply);
}

function answerQueue(flow: Flow, gen: string): void {
    const queued = flow.calls.filter((c) => c.method === "QueueWorldCommand").pop();
    assert.ok(queued !== undefined, "expected queue filing before second capture");
    const body = JSON.parse((queued as Call).payload) as Record<string, unknown>;
    (queued as Call).callback(JSON.stringify({
        v: 4, outcome: "queued", gen,
        client: body["client"], clientCmd: body["clientCmd"], globalCmd: "1",
    }));
}

function writeCorrelation(flow: Flow): string {
    const body = JSON.parse(flow.lastCall("WriteFloatIntent").payload) as Record<string, unknown>;
    return body["correlation_id"] as string;
}

function storedWriteReply(flow: Flow, stored: number): string {
    return JSON.stringify({ v: 2, correlation_id: writeCorrelation(flow), outcome: "stored", stored });
}

function answerRead(flow: Flow, records: ReadonlyArray<{ win: string; data: string }>): void {
    const read = flow.lastCall("ReadFloatIntent");
    const corr = (JSON.parse(read.payload) as Record<string, unknown>)["correlation_id"] as string;
    read.callback(JSON.stringify({
        v: 2, correlation_id: corr, outcome: "ok",
        records, stored: records.length, returned: records.length,
    }));
}

function firstCapture(flow: Flow): Record<string, unknown> {
    const capture = flow.lastCall("DescribeWorld");
    return JSON.parse(capture.payload) as Record<string, unknown>;
}

function innerIntentRecords(flow: Flow): unknown {
    const body = firstCapture(flow);
    return (body["report"] as Record<string, unknown>)["intentRecords"];
}

describe("world entry opaque intent persistence", () => {
    it("gates the first capture on hydration; degraded reads still dispatch", () => {
        const flow = startFlow();
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        assert.equal(flow.hasCall("DescribeWorld"), false);
        answerRead(flow, []);
        flow.pump(120);
        assert.equal(flow.hasCall("DescribeWorld"), true);
        assert.deepEqual(innerIntentRecords(flow), []);
        for (const line of flow.logs) {
            assert.ok(!line.includes("secret"), `leak: ${line}`);
        }
        flow.handle.stop();
    });

    it("imported records ride the first report live-joined with exact bytes", () => {
        const flow = startFlow();
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_GHOST },
        ]);
        flow.pump(120);
        assert.deepEqual(innerIntentRecords(flow), [{ win: "win-a", data: DATA_A }]);
        flow.handle.stop();
    });

    it("accepted core intentWrites replace and delete, then persist the full snapshot", () => {
        const flow = startFlow();
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [
            { win: "win-a", data: DATA_A },
            { win: "win-b", data: DATA_B },
            { win: "ghost", data: DATA_GHOST },
        ]);
        flow.pump(120);
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
            { win: "win-b", data: null },
        ]));
        const write = flow.lastCall("WriteFloatIntent");
        const body = JSON.parse(write.payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.deepEqual(body["records"], [
            { win: "win-a", data: DATA_B },
            { win: "ghost", data: DATA_GHOST },
        ]);
        write.callback(storedWriteReply(flow, 2));
        for (const line of flow.logs) {
            assert.ok(!line.includes(DATA_A) && !line.includes(DATA_B), `bytes leaked: ${line}`);
        }
        flow.handle.stop();
    });

    it("accepted replies without writes persist nothing and keep the map", () => {
        const flow = startFlow();
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, []));
        assert.equal(flow.hasCall("WriteFloatIntent"), false);
        flow.handle.stop();
    });

    it("malformed intentWrites refuse the reply without touching persistence", () => {
        const flow = startFlow();
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const envelope = firstCapture(flow);
        const malformed = buildAccept("gen-1", envelope, [
            { win: "win-a", data: DATA_A },
            { win: "win-a", data: DATA_B },
        ]);
        assert.equal(parseWorldReply(malformed), null, "duplicate intent subject refuses at the parser");
        flow.lastCall("DescribeWorld").callback(malformed);
        assert.equal(flow.hasCall("WriteFloatIntent"), false);
        assert.ok(flow.logs.some((l) => l.includes("reply-malformed")), flow.logs.join("\n"));
        flow.handle.stop();
    });

    it("adapter restart rereads the persisted snapshot into its first report", () => {
        const bus = {
            stored: [{ win: "win-a", data: DATA_A }],
            degradeReads: false,
        };
        const first = startFlow(bus);
        answerHandshake(first);
        answerSettings(first);
        first.pump(120);
        const envelope = firstCapture(first);
        first.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ]));
        assert.deepEqual(bus.stored, [{ win: "win-a", data: DATA_B }]);
        first.handle.stop();
        const second = startFlow(bus);
        answerHandshake(second);
        answerSettings(second);
        second.pump(120);
        assert.deepEqual(innerIntentRecords(second), [{ win: "win-a", data: DATA_B }]);
        second.handle.stop();
    });

    it("withholds dependent native writes until the correlated persist succeeds", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ]));
        assert.equal(flow.hasCall("WriteFloatIntent"), true);
        // The persist is still owed: no dependent apply ran yet.
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        // A foreign correlation never settles the owed write.
        answerWrite(flow, JSON.stringify({ v: 2, correlation_id: "intent-write-999", outcome: "stored", stored: 1 }));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        // The correlated success releases the dependent apply.
        answerWrite(flow, storedWriteReply(flow, 1));
        assert.equal(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore + 1,
        );
        flow.handle.stop();
    });

    it("failed persistence applies nothing and reports once", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ]));
        answerWrite(flow, JSON.stringify({
            v: 2, correlation_id: writeCorrelation(flow), outcome: "rejected", reason: "unauthorized",
        }));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.ok(flow.logs.some((l) => l.includes("intent-write-failed")), flow.logs.join("\n"));
        // No dependent follow-up dispatch runs from the failed accept.
        assert.equal(flow.calls.filter((c) => c.method === "DescribeWorld").length, 1);
        flow.handle.stop();
    });

    it("failed persist keeps owed status: unchanged next accept withholds apply until covering stored success", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const geometryBefore = JSON.stringify(flow.geometryOf("win-a"));
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ], "1"));
        assert.equal(flow.hasCall("WriteFloatIntent"), true);
        assert.equal(flow.calls.filter((c) => c.method === "WriteFloatIntent").length, 1);
        answerWrite(flow, JSON.stringify({
            v: 2, correlation_id: writeCorrelation(flow), outcome: "rejected", reason: "unauthorized",
        }));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        // Drive the next acceptance through a real queued command: the
        // serial snapshot path files, dispatches, and accepts capture #2.
        flow.handle.requestFocus("left");
        flow.pump(120);
        answerQueue(flow, "gen-1");
        flow.pump(120);
        const describes = flow.calls.filter((c) => c.method === "DescribeWorld");
        assert.ok(describes.length >= 2, `expected second capture, got ${describes.length}`);
        const second = JSON.parse(flow.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", second, [], "2"));
        flow.lastCall("TakeCommandResult").callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
        // Owed map: the unchanged accept upgrades to a covering write and
        // withholds native effects (no apply, geometry untouched).
        assert.equal(flow.calls.filter((c) => c.method === "WriteFloatIntent").length, 2);
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        // The covering stored success releases the dependent apply exactly once.
        answerWrite(flow, storedWriteReply(flow, 1));
        assert.equal(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore + 1,
        );
        flow.handle.stop();
    });

    it("older accepted reply never applies after a newer accepted rev on the same watermark", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const geometryBefore = JSON.stringify(flow.geometryOf("win-a"));
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ], "1"));
        assert.equal(flow.calls.filter((c) => c.method === "WriteFloatIntent").length, 1);
        // Newer accepted reply lands while the first persist is still owed.
        flow.handle.requestFocus("left");
        flow.pump(120);
        answerQueue(flow, "gen-1");
        flow.pump(120);
        assert.ok(flow.calls.filter((c) => c.method === "DescribeWorld").length >= 2);
        const second = JSON.parse(flow.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", second, [
            { win: "win-a", data: DATA_A },
        ], "2"));
        flow.lastCall("TakeCommandResult").callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
        // Succeeding the older covering write must not apply: rev 1 is
        // superseded by accepted rev 2 even on the same generation.
        const firstWrite = flow.calls.filter((c) => c.method === "WriteFloatIntent")[0] as { payload: string };
        const firstCorr = (JSON.parse(firstWrite.payload) as Record<string, unknown>)["correlation_id"] as string;
        flow.lastCall("WriteFloatIntent").callback(JSON.stringify({ v: 2, correlation_id: firstCorr, outcome: "stored", stored: 1 }));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        // The newer covering write still applies on its own stored success.
        answerWrite(flow, storedWriteReply(flow, 1));
        assert.equal(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore + 1,
        );
        flow.handle.stop();
    });
    it("stop during an owed persist settles nothing further", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ]));
        flow.handle.stop();
        answerWrite(flow, storedWriteReply(flow, 1));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
    });
    it("stale settings snapshot withholds the dependent apply until the covering settings accept", () => {
        const flow = startFlow({ stored: [], degradeReads: false, holdWrites: true });
        answerHandshake(flow);
        answerSettings(flow);
        flow.pump(120);
        answerRead(flow, [{ win: "win-a", data: DATA_A }]);
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const geometryBefore = JSON.stringify(flow.geometryOf("win-a"));
        const envelope = firstCapture(flow);
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
        ], "1"));
        assert.equal(flow.calls.filter((c) => c.method === "WriteFloatIntent").length, 1);
        // Live settings injection: changed gap bumps the revision on the
        // configChanged edge; the old intent waiter still carries rev1.
        const settingsBefore = flow.calls.filter((c) => {
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
            } catch (error) {
                void error;
                return false;
            }
        }).length;
        flow.setGap(12);
        flow.fireConfigChanged();
        const settingsSend = flow.lastCall("DiscoverWorld", "settings");
        const settingsBody = JSON.parse(settingsSend.payload) as Record<string, unknown>;
        assert.notEqual(settingsBody["rev"], "1", "changed gap bumps the settings rev");
        assert.equal(flow.calls.filter((c) => {
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
            } catch (error) {
                void error;
                return false;
            }
        }).length, settingsBefore + 1, "exactly one settings resend");
        // Accept the newer settings before the older persist succeeds: the
        // older waiter is stale and must never authorize native writes.
        settingsSend.callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter",
            gen: "gen-1", req: settingsBody["req"], rev: settingsBody["rev"],
            status: "accepted", diagnostics: [],
        }));
        answerWrite(flow, storedWriteReply(flow, 1));
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        flow.handle.stop();
    });
    it("superseded accepted intentWrites still persist with no native apply", () => {
        const bus = {
            stored: [
                { win: "win-a", data: DATA_A },
                { win: "win-b", data: DATA_B },
                { win: "ghost", data: DATA_GHOST },
            ],
            degradeReads: false, holdWrites: true,
        };
        const flow = startFlow(bus);
        answerHandshake(flow);
        answerSettings(flow);
        // The bus auto-answers ReadFloatIntent from bus.stored, so no
        // manual answerRead: a late second callback would not reseed the
        // already-hydrated map and would drop the out-of-scope ghost.
        flow.pump(120);
        const appliedBefore = flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length;
        const geometryBefore = JSON.stringify(flow.geometryOf("win-a"));
        const envelope = firstCapture(flow);
        // Native input lands during flight: the watermark advances before
        // the accepted reply arrives, so the observation is superseded.
        flow.fireConfigChanged();
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", envelope, [
            { win: "win-a", data: DATA_B },
            { win: "win-b", data: null },
        ], "1"));
        assert.ok(flow.logs.some((l) => l.includes("superseded-during-flight")), flow.logs.join("\n"));
        // The superseded accept still persists the exact core writes.
        assert.equal(flow.calls.filter((c) => c.method === "WriteFloatIntent").length, 1);
        const writeBody = JSON.parse(flow.lastCall("WriteFloatIntent").payload) as Record<string, unknown>;
        assert.deepEqual(writeBody["records"], [
            { win: "win-a", data: DATA_B },
            { win: "ghost", data: DATA_GHOST },
        ]);
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        // The correlated persist success stays durable but applies nothing:
        // the waiter carries the superseded watermark.
        answerWrite(flow, storedWriteReply(flow, 2));
        assert.deepEqual(bus.stored, [
            { win: "win-a", data: DATA_B },
            { win: "ghost", data: DATA_GHOST },
        ]);
        assert.deepEqual(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore,
        );
        assert.equal(JSON.stringify(flow.geometryOf("win-a")), geometryBefore);
        // Follow-up fresh capture carries the persisted intent live-joined,
        // and its unchanged acceptance applies normally.
        flow.pump(120);
        const second = JSON.parse(flow.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        assert.deepEqual(
            (second["report"] as Record<string, unknown>)["intentRecords"],
            [{ win: "win-a", data: DATA_B }],
        );
        flow.lastCall("DescribeWorld").callback(acceptCapture("gen-1", second, [], "2"));
        assert.equal(
            flow.logs.filter((l) => l.includes("omnitiler:world:applied")).length,
            appliedBefore + 1,
        );
        flow.handle.stop();
        // Restart rereads the persisted snapshot into its first report.
        const revived = startFlow(bus);
        answerHandshake(revived);
        answerSettings(revived);
        revived.pump(120);
        assert.deepEqual(innerIntentRecords(revived), [{ win: "win-a", data: DATA_B }]);
        revived.handle.stop();
    });
});
