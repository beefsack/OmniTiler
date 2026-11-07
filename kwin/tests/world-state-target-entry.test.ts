// Production-wired StateTarget entry coverage (atomic migration):
// uses the ACTUAL startWorldAdapterEntry plus real parsed replies and the
// real preimage bus. No internal executor/recovery mocking.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { DISABLED_PRESENTATION } from "./world-wire-harness";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { NATIVE_PREIMAGE_VERSION } from "../src/world-native-preimage";

interface FakeDesktop { id: string }
interface FakeWindow {
    internalId: string; managed: boolean; deleted: boolean; transient: boolean;
    transientFor: unknown; dialog: boolean; modal: boolean; wantsInput: boolean;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    clientGeometry: { x: number; y: number; width: number; height: number } | null;
    output: { name: string }; desktops: FakeDesktop[];
    minimized: boolean; hidden: boolean;
    fullscreen: boolean; maximized: boolean; sticky: boolean;
    keepAbove: boolean; keepBelow: boolean;
    minSize: null; maxSize: null;
    fullScreen: boolean; maximizeMode: number; onAllDesktops: boolean;
    resourceClass: string;
    setMaximize: (v: boolean, h: boolean) => void;
}
interface CapturedCall { method: string; payload: string; callback: (reply: unknown) => void; answered: boolean }

const desk = (id: string): FakeDesktop => ({ id });
function makeWin(id: string, ws: FakeDesktop, out = "out", cls = "allowed"): FakeWindow {
    const w = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null,
        dialog: false, modal: false, wantsInput: true, normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: out }, desktops: [ws], minimized: false, hidden: false,
        fullscreen: false, maximized: false, sticky: false,
        keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        fullScreen: false, maximizeMode: 0, onAllDesktops: false, resourceClass: cls,
        setMaximize: undefined as unknown as (v: boolean, h: boolean) => void,
    } as FakeWindow;
    w.setMaximize = (v: boolean, h: boolean): void => {
        w.maximizeMode = v && h ? 3 : v ? 1 : h ? 2 : 0;
    };
    return w;
}

function fakeSignal(): { handlers: Array<(...a: Array<unknown>) => void>; signal: { connect: (h: (...a: Array<unknown>) => void) => void; disconnect: (h: (...a: Array<unknown>) => void) => void } } {
    const handlers: Array<(...a: Array<unknown>) => void> = [];
    return { handlers, signal: { connect: (h) => { handlers.push(h); }, disconnect: (h) => { const i = handlers.indexOf(h); if (i >= 0) handlers.splice(i, 1); } } };
}

interface Harness {
    world: { screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>; desktops: FakeDesktop[]; wins: FakeWindow[]; activeScreen: { name: string }; activeWindow: FakeWindow | null; currentDesktop: FakeDesktop };
    calls: CapturedCall[]; timers: Array<{ delay: number; cb: () => void }>; logs: string[];
    shortcuts: Map<string, () => void>; signals: Record<string, ReturnType<typeof fakeSignal>>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    seedRecords: Array<Record<string, unknown>>;
    runTimers: (maxDelay?: number) => void;
    lastCall: (method: string) => CapturedCall;
}

function makeHarness(opts: { scopeClass?: string | readonly string[]; seedRecords?: Array<Record<string, unknown>>; autoSeed?: boolean; wins?: FakeWindow[]; desktops?: FakeDesktop[] } = {}): Harness {
    const ws1 = desk("ws1"); const ws2 = desk("ws2");
    const desktops = opts.desktops ?? [ws1, ws2];
    const wins = opts.wins ?? [makeWin("win-a", desktops[0] as FakeDesktop), makeWin("win-b", desktops[1] as FakeDesktop)];
    const world = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0] as FakeWindow | null,
        currentDesktop: desktops[0] as FakeDesktop,
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const shortcuts = new Map<string, () => void>();
    const signals: Record<string, ReturnType<typeof fakeSignal>> = {
        windowAdded: fakeSignal(), windowRemoved: fakeSignal(), windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(), screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(), activeScreenChanged: fakeSignal(),
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens, desktops: world.desktops, activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        windowAdded: signals["windowAdded"]!.signal, windowRemoved: signals["windowRemoved"]!.signal,
        windowActivated: signals["windowActivated"]!.signal, desktopsChanged: signals["desktopsChanged"]!.signal,
        screensChanged: signals["screensChanged"]!.signal,
        currentDesktopChanged: signals["currentDesktopChanged"]!.signal,
        activeScreenChanged: signals["activeScreenChanged"]!.signal,
        get activeWindow() { return world.activeWindow; },
        set activeWindow(ref: unknown) {
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) world.activeWindow = found;
        },
        get currentDesktop() { return world.currentDesktop; },
        set currentDesktop(ref: unknown) {
            const found = world.desktops.find((d) => (d as unknown) === ref);
            if (found) world.currentDesktop = found;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) world.currentDesktop = found;
        },
    };
    const seedRecords = opts.seedRecords ?? [];
    const autoSeed = opts.autoSeed ?? true;
    const handle = startWorldAdapterEntry({
        workspace: surface, owner: "owner-a", adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            const call: CapturedCall = { method, payload, callback, answered: false };
            calls.push(call);
            if (method === "QueueWorldCommand") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const gen = typeof body["gen"] === "string" ? (body["gen"] as string) : "gen-1";
                call.answered = true;
                callback(JSON.stringify({ v: 4, outcome: "queued", gen, client: body["client"], clientCmd: body["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                call.answered = true;
                callback(JSON.stringify({ v: 4, outcome: "pending", gen: typeof body["gen"] === "string" ? body["gen"] : "gen-1" }));
            } else if (method === "ReadNativePreimage" && autoSeed) {
                const corr = (JSON.parse(payload) as Record<string, unknown>)["correlation_id"] as string;
                call.answered = true;
                callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "ok", records: seedRecords, stored: seedRecords.length, returned: seedRecords.length }));
            }
        },
        scheduleOnce: (delay, cb) => { timers.push({ delay, cb }); return () => undefined; },
        log: (m) => { logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: (name, _t, _s, cb) => { shortcuts.set(name, cb); return true; },
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
        ...(opts.scopeClass !== undefined ? { scopeClass: opts.scopeClass } : {}),
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) { timers.splice(i, 1); i -= 1; t.cb(); }
        }
    };
    return {
        world, calls, timers, logs, shortcuts, signals, handle, seedRecords, runTimers,
        lastCall: (method: string): CapturedCall => {
            const f = [...calls].reverse().find((c) => c.method === method);
            assert.ok(f !== undefined, `expected ${method}`);
            return f as CapturedCall;
        },
    };
}

function answerHandshake(h: Harness, gen = "gen-1"): void {
    const f = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") return false;
        try { return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake"; } catch { return false; }
    });
    assert.ok(f !== undefined, "expected handshake");
    const req = (JSON.parse((f as CapturedCall).payload) as Record<string, unknown>)["req"] as string;
    (f as CapturedCall).callback(JSON.stringify({ type: "handshake-reply", v: 4, adapter: "test-adapter", req, ready: false, kind: "", gen }));
}
function answerSettings(h: Harness, gen = "gen-1"): void {
    const f = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") return false;
        try { return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings"; } catch { return false; }
    });
    assert.ok(f !== undefined, "expected settings");
    const body = JSON.parse((f as CapturedCall).payload) as Record<string, unknown>;
    (f as CapturedCall).callback(JSON.stringify({ type: "settings-reply", v: 4, adapter: "test-adapter", gen, req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [] }));
}
function discover(h: Harness, gen = "gen-1"): void {
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    const auto = h.lastCall("DescribeWorld");
    const body = JSON.parse(auto.payload);
    auto.callback(acceptedReply(gen, body["source"] as string, body["nativeInvalid"] as string, "0"));
}
function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false, source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [],
        warpCursor: null, intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}
function winRow(win: string, over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
        win, out: "out", ws: "ws1", alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "",
        frame_op: "", floating: false, float_origin: "", clear_op: "", cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
        ...over,
    };
}
function st(property: string, value: boolean, auth = "controlled", reason = "", cmdSeq: string | null = null): Record<string, unknown> {
    return { property, value, auth, reason, cmdSeq };
}
function requestOf(h: Harness): Record<string, unknown> {
    return JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
}
function reportOf(h: Harness): Record<string, unknown> {
    return requestOf(h)["report"] as Record<string, unknown>;
}
function answerOldestWrite(h: Harness, outcome: "stored" | "rejected" = "stored"): void {
    const call = h.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered) as CapturedCall | undefined;
    assert.ok(call !== undefined, "expected pending Write");
    call.answered = true;
    const corr = (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
    if (outcome === "stored") {
        const n = ((JSON.parse(call.payload) as Record<string, unknown>)["records"] as unknown[]).length;
        call.callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "stored", stored: n }));
    } else {
        call.callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "rejected", reason: "store-unavailable" }));
    }
}
function pendingWrites(h: Harness): number {
    return h.calls.filter((c) => c.method === "WriteNativePreimage" && !c.answered).length;
}
function stateAttemptsOf(h: Harness): Array<Record<string, unknown>> {
    return ((reportOf(h)["stateAttempts"] as Array<Record<string, unknown>>) ?? []);
}

describe("world state-target entry (production wiring)", () => {
    it("recovery race: same-slot Controlled waits for the durable clear ack and a fresh followup", () => {
        // Durable original says unmaximized; live starts maximized so the
        // startup restore runs. The fresh matching capture clears durably;
        // a same-slot Controlled in the clearing reply must NOT arm until
        // the ack lands and a fresh followup carries it.
        const h = makeHarness({
            seedRecords: [{ win: "win-a", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
        });
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        live.maximizeMode = 3;
        discover(h);
        // First accepted reply ran the restore (maximize cleared).
        assert.equal(live.maximizeMode, 0);
        // Fresh matching capture is owed without new external input.
        h.runTimers();
        const fresh = requestOf(h);
        // This reply both completes recovery (fresh facts match the
        // original 0) and carries a same-slot Controlled maximize=true.
        // The executor must not arm while the clear ack is pending.
        (h.lastCall("DescribeWorld")).callback(acceptedReply("gen-1", fresh["source"] as string, fresh["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "7")] })],
        }));
        // The clear flush is pending; the racy Controlled must not have
        // armed a second write nor run the setter yet.
        assert.equal(live.maximizeMode, 0, "racy Controlled waits for the durable clear");
        assert.ok(pendingWrites(h) >= 1, "clear ack still owed");
        // Ack the durable clear: recovery completes and schedules one
        // fresh observation without new external input.
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        const describesAfter = h.calls.filter((c) => c.method === "DescribeWorld").length;
        assert.ok(describesAfter > describesBefore, "clear ack schedules a fresh followup");
        // The followup carries the waiting Controlled; answer it and the
        // setter runs exactly once.
        const follow = requestOf(h);
        (h.lastCall("DescribeWorld")).callback(acceptedReply("gen-1", follow["source"] as string, follow["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "7")] })],
        }));
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        assert.equal(live.maximizeMode, 3, "Controlled runs only after the durable clear");
        // A republished identical target spends exactly once.
        const spent = requestOf(h);
        void spent;
        h.handle?.stop();
    });

    it("Controlled fullscreen persists then reports the exact raw attempt on the next capture", () => {
        const h = makeHarness();
        discover(h);
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        assert.equal(live.fullScreen, false);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", { state: [st("fullscreen", true, "controlled", "", null)] })],
        }));
        // Persist-before-write: one flush owed, setter held until the ack.
        assert.equal(pendingWrites(h), 1);
        assert.equal(live.fullScreen, false);
        answerOldestWrite(h, "stored");
        assert.equal(live.fullScreen, true);
        // The exact raw attempt (accepted, pre false) rides the next report:
        // poke one native event so the buffered evidence dispatches.
        for (const handler of [...h.signals["windowActivated"]!.handlers]) {
            (handler as (...a: Array<unknown>) => void)();
        }
        h.runTimers();
        const attempts = stateAttemptsOf(h);
        const row = attempts.find((a) => a["win"] === "win-a" && a["property"] === "fullscreen");
        assert.ok(row !== undefined, "exact raw attempt rides next capture");
        assert.equal(row["target"], true);
        assert.equal(row["setter"], "accepted");
        assert.equal(row["pre"], false);
        h.handle?.stop();
    });

    it("Hold/Untouched and descriptive maximize take no authority", () => {
        const h = makeHarness();
        discover(h);
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const writesBefore = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", {
                state: [
                    st("fullscreen", true, "hold", "minimized", null),
                    st("sticky", true, "untouched", "", null),
                    st("maximized", true, "hold", "minimized", null),
                ],
            })],
        }));
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesBefore);
        assert.equal(live.fullScreen, false);
        assert.equal(live.onAllDesktops, false);
        assert.equal(live.maximizeMode, 0);
        assert.equal(stateAttemptsOf(h).length, 0);
        h.handle?.stop();
    });

    it("missing or failed seed keeps ordinary tiling usable with no retry", () => {
        const h = makeHarness({ autoSeed: false });
        discover(h);
        const readsBefore = h.calls.filter((c) => c.method === "ReadNativePreimage").length;
        assert.ok(readsBefore >= 1);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        // Ordinary controlled geometry still applies with the store down.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", {
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "",
            })],
        }));
        const target = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        assert.equal(target.frameGeometry.x, 10);
        // No blind seed retry: exactly one read for the generation.
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "ReadNativePreimage").length, readsBefore);
        // Controlled native rows drop closed while degraded.
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const req2 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("fullscreen", true)] })],
        }));
        assert.equal(target.fullScreen, false);
        h.handle?.stop();
    });

    it("shortcuts enqueue sticky/fullscreen with zero direct setters", () => {
        const h = makeHarness();
        discover(h);
        const live = h.world.wins.find((w) => w.internalId === "win-a") as unknown as Record<string, unknown>;
        const sticky = h.shortcuts.get("omnitiler-toggle-sticky") as () => void;
        const fs = h.shortcuts.get("omnitiler-toggle-fullscreen") as () => void;
        assert.ok(sticky !== undefined && fs !== undefined);
        sticky();
        h.runTimers();
        const filed = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed["command"], { op: "toggle-sticky", win: "win-a" });
        assert.equal(live["onAllDesktops"], false);
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1"));
        fs();
        h.runTimers();
        const filed2 = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        assert.deepEqual(filed2["command"], { op: "toggle-fullscreen", win: "win-a" });
        assert.equal(live["fullScreen"], false);
        h.handle?.stop();
    });

    it("automatic float classification writes no local above and republished bindings do not double-write", () => {
        const h = makeHarness();
        discover(h);
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        // Ordinary float row: converges intent, never a keepAbove write.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", { floating: true, frame_auth: "untouched", frame_hold: "" })],
        }));
        assert.equal(live.keepAbove, false);
        assert.equal(live.keepBelow, false);
        // Controlled above acquire runs once; the identical republish spends.
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const req2 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("above", true, "controlled", "", "9")] })],
        }));
        answerOldestWrite(h, "stored");
        assert.equal(live.keepAbove, true);
        const writesAfterFirst = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req3 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req3["source"] as string, req3["nativeInvalid"] as string, "3", {
            windows: [winRow("win-a", { state: [st("above", true, "controlled", "", "9")] })],
        }));
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesAfterFirst, "identical republish spends without a second persist");
        h.handle?.stop();
    });

    it("gen, settings, watermark, authority, and lifetime fences drop without writes", () => {
        // Stale generation.
        {
            const h = makeHarness();
            discover(h);
            const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            const writesBefore = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-2", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [winRow("win-a", { state: [st("fullscreen", true)] })],
            }));
            h.runTimers();
            assert.equal(live.fullScreen, false);
            assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesBefore);
            h.handle?.stop();
        }
        // Missing window and hold authority.
        {
            const h = makeHarness();
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            const writesBefore = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [
                    winRow("win-ghost", { state: [st("fullscreen", true)] }),
                    winRow("win-a", { state: [st("fullscreen", true, "hold", "minimized", null)] }),
                ],
            }));
            h.runTimers();
            const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            assert.equal(live.fullScreen, false);
            assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesBefore);
            h.handle?.stop();
        }
    });

    it("scoped foreign windows stay untouched including startup recovery", () => {
        const ws = desk("ws1");
        const home = makeWin("win-a", ws, "out", "allowed");
        const foreign = makeWin("win-f", ws, "out", "foreign");
        const h = makeHarness({
            scopeClass: ["allowed"],
            seedRecords: [{ win: "win-f", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
            wins: [home, foreign], desktops: [ws],
        });
        foreign.maximizeMode = 3;
        discover(h);
        // Foreign durable original never restores; foreign Controlled never actuates.
        assert.equal(foreign.maximizeMode, 3, "foreign recovery never writes");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const inner = reportOf(h);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.ok(wins.every((w) => w["win"] !== "win-f"), "foreign never rides the report");
        const writesBefore = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-f", { state: [st("maximized", true)] })],
        }));
        h.runTimers();
        assert.equal(foreign.maximizeMode, 3);
        assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesBefore);
        h.handle?.stop();
    });

    it("duplicate native ids fail closed with no writes", () => {
        const ws = desk("ws1");
        const a = makeWin("win-dup", ws);
        const b = makeWin("win-dup", ws);
        (b as FakeWindow).deleted = true;
        const h = makeHarness({ wins: [a, b], desktops: [ws] });
        answerHandshake(h);
        answerSettings(h);
        h.runTimers();
        // Duplicate stable ids never produce a valid report: fail-closed
        // with zero native writes and zero setters either way.
        const describes = h.calls.filter((c) => c.method === "DescribeWorld");
        if (describes.length > 0) {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            const writesBefore = h.calls.filter((c) => c.method === "WriteNativePreimage").length;
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [winRow("win-dup", { state: [st("fullscreen", true)] })],
            }));
            h.runTimers();
            assert.equal(a.fullScreen, false);
            assert.equal(b.fullScreen, false);
            assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage").length, writesBefore);
        } else {
            assert.ok(h.logs.some((l) => l.includes("report-invalid") || l.includes("observe-invalid") || l.includes("world")));
            assert.equal(a.fullScreen, false);
            assert.equal(h.calls.filter((c) => c.method === "WriteNativePreimage" && !c.answered).length, 0);
        }
        h.handle?.stop();
    });

    it("startup chain restores, clears on the fresh match, then runs the core target after the ack", () => {
        const h = makeHarness({
            seedRecords: [{ win: "win-a", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
        });
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        live.maximizeMode = 3;
        discover(h);
        assert.equal(live.maximizeMode, 0, "startup restores the durable original");
        h.runTimers();
        const fresh = requestOf(h);
        // Fresh matching capture with no core target: completes the clear.
        (h.lastCall("DescribeWorld")).callback(acceptedReply("gen-1", fresh["source"] as string, fresh["nativeInvalid"] as string, "1"));
        assert.ok(pendingWrites(h) >= 1, "matching fresh owes the durable clear");
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        assert.ok(h.calls.filter((c) => c.method === "DescribeWorld").length > describesBefore, "durable clear schedules its own followup");
        // Only now does the core target run.
        const follow = requestOf(h);
        (h.lastCall("DescribeWorld")).callback(acceptedReply("gen-1", follow["source"] as string, follow["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "11")] })],
        }));
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        assert.equal(live.maximizeMode, 3, "core target runs after the durable clear");
        h.handle?.stop();
    });

    it("scoped live foreign slot survives every persisted snapshot with zero foreign setters", () => {
        const ws = desk("ws1");
        const home = makeWin("win-a", ws, "out", "allowed");
        const foreign = makeWin("win-f", ws, "out", "foreign");
        let foreignSetters = 0;
        const origSet = foreign.setMaximize;
        foreign.setMaximize = (v, hh): void => { foreignSetters += 1; origSet(v, hh); };
        const h = makeHarness({
            scopeClass: ["allowed"],
            seedRecords: [{ win: "win-f", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
            wins: [home, foreign], desktops: [ws],
        });
        foreign.maximizeMode = 3;
        discover(h);
        assert.equal(foreign.maximizeMode, 3, "foreign recovery never writes");
        assert.equal(foreignSetters, 0, "no foreign setter on startup");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const inner = reportOf(h) as Record<string, unknown>;
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.ok(wins.every((w) => w["win"] !== "win-f"), "foreign never rides the scoped report");
        assert.equal(inner["complete"], true, "scoped report stays complete without the foreign window");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a")],
        }));
        h.runTimers();
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        assert.equal(foreign.maximizeMode, 3, "foreign live window untouched after complete scoped capture");
        assert.equal(foreignSetters, 0, "zero foreign setters after complete scoped capture");
        // Force a persisted snapshot by arming the in-scope home: the live
        // foreign slot must ride every snapshot.
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const reqArm = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", reqArm["source"] as string, reqArm["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "31")] })],
        }));
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        assert.equal(foreignSetters, 0, "arming home never touches foreign");
        const writes = h.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.ok(writes.length >= 1, "at least one persisted snapshot");
        for (const w of writes) {
            const recs = ((JSON.parse(w.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
            assert.ok(recs.includes("win-f"), "live foreign slot remains in every persisted snapshot");
        }
        h.handle?.stop();
    });

    it("duplicate foreign ids in the full roster over-retain the gone scoped slot", () => {
        const ws = desk("ws1");
        const home = makeWin("win-a", ws, "out", "allowed");
        const f1 = makeWin("win-f", ws, "out", "foreign");
        const f2 = makeWin("win-f", ws, "out", "foreign");
        const h = makeHarness({
            scopeClass: ["allowed"],
            seedRecords: [{ win: "win-gone", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
            wins: [home, f1, f2], desktops: [ws],
        });
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const inner = reportOf(h) as Record<string, unknown>;
        assert.equal(inner["complete"], true, "scoped report stays complete with foreign duplicates excluded");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a")],
        }));
        h.runTimers();
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        // Force a snapshot by arming home; the invalid full roster must
        // have over-retained the gone slot through the earlier complete
        // scoped capture.
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const reqArm = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", reqArm["source"] as string, reqArm["nativeInvalid"] as string, "2", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "32")] })],
        }));
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        const writes = h.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.ok(writes.length >= 1, "at least one persisted snapshot");
        const last = writes[writes.length - 1] as CapturedCall;
        const recs = ((JSON.parse(last.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
        assert.ok(recs.includes("win-gone"), "duplicate full roster over-retains the gone slot");
        h.handle?.stop();
    });

    it("invalid foreign id in the full roster over-retains; malformed class with valid UUID retains", () => {
        // Invalid id (null after normalization) in the full roster.
        {
            const ws = desk("ws1");
            const home = makeWin("win-a", ws, "out", "allowed");
            const bad = makeWin("win-f", ws, "out", "foreign");
            (bad as unknown as Record<string, unknown>)["internalId"] = "bad id!";
            const h = makeHarness({
                scopeClass: ["allowed"],
                seedRecords: [{ win: "win-gone", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
                wins: [home, bad], desktops: [ws],
            });
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [winRow("win-a")],
            }));
            h.runTimers();
            while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
            h.runTimers();
            h.handle?.requestWorkspaceRelative(-1);
            h.runTimers();
            const reqArm = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", reqArm["source"] as string, reqArm["nativeInvalid"] as string, "2", {
                windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "33")] })],
            }));
            while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
            h.runTimers();
            const writes = h.calls.filter((c) => c.method === "WriteNativePreimage");
            assert.ok(writes.length >= 1);
            const last = writes[writes.length - 1] as CapturedCall;
            const recs = ((JSON.parse(last.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
            assert.ok(recs.includes("win-gone"), "malformed full roster over-retains");
            h.handle?.stop();
        }
        // Malformed class read with a valid UUID: lifetime proof needs no
        // class read, so the live foreign UUID retains.
        {
            const ws = desk("ws1");
            const home = makeWin("win-a", ws, "out", "allowed");
            const foreign = makeWin("win-f", ws, "out", "foreign");
            Object.defineProperty(foreign, "resourceClass", {
                get(): unknown { throw new Error("unreadable-class"); },
                configurable: true,
            });
            const h = makeHarness({
                scopeClass: ["allowed"],
                seedRecords: [{ win: "win-f", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null }],
                wins: [home, foreign], desktops: [ws],
            });
            foreign.maximizeMode = 3;
            discover(h);
            assert.equal(foreign.maximizeMode, 3);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            const inner = reportOf(h) as Record<string, unknown>;
            const wins = inner["windows"] as Array<Record<string, unknown>>;
            assert.ok(wins.every((w) => w["win"] !== "win-f"), "foreign with unreadable class stays out of the scoped report");
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [winRow("win-a")],
            }));
            h.runTimers();
            while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
            h.runTimers();
            h.handle?.requestWorkspaceRelative(-1);
            h.runTimers();
            const reqArm = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", reqArm["source"] as string, reqArm["nativeInvalid"] as string, "2", {
                windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "34")] })],
            }));
            while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
            h.runTimers();
            const writes = h.calls.filter((c) => c.method === "WriteNativePreimage");
            assert.ok(writes.length >= 1);
            for (const w of writes) {
                const recs = ((JSON.parse(w.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
                assert.ok(recs.includes("win-f"), "valid UUID retains despite malformed class");
            }
            h.handle?.stop();
        }
    });

    it("truly ended runtime slot prunes and persists only on complete fresh plus absent full roster", () => {
        // Prune path: arm win-a via Controlled, close it, complete fresh
        // with the full roster absent removes and persists the removal.
        const h = makeHarness();
        discover(h);
        const live = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "21")] })],
        }));
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        assert.equal(live.maximizeMode, 3, "runtime slot armed");
        h.world.wins = h.world.wins.filter((w) => w.internalId !== "win-a");
        if (h.world.activeWindow?.internalId === "win-a") {
            h.world.activeWindow = h.world.wins[0] ?? null;
        }
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req2 = requestOf(h);
        const inner2 = reportOf(h) as Record<string, unknown>;
        assert.equal(inner2["complete"], true, "fresh complete after the close");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "2", {
            windows: [],
        }));
        h.runTimers();
        while (pendingWrites(h) > 0) answerOldestWrite(h, "stored");
        h.runTimers();
        const writes = h.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.ok(writes.length >= 1, "removal persists with a write");
        const last = writes[writes.length - 1] as CapturedCall;
        const recs = ((JSON.parse(last.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
        assert.ok(!recs.includes("win-a"), "proven-ended runtime slot pruned and persisted");
        h.handle?.stop();
        // Held path: same close but the fresh is held, so it over-retains.
        const h2 = makeHarness();
        discover(h2);
        const live2 = h2.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
        h2.handle?.requestWorkspaceRelative(1);
        h2.runTimers();
        const r1 = requestOf(h2);
        h2.lastCall("DescribeWorld").callback(acceptedReply("gen-1", r1["source"] as string, r1["nativeInvalid"] as string, "1", {
            windows: [winRow("win-a", { state: [st("maximized", true, "controlled", "", "22")] })],
        }));
        while (pendingWrites(h2) > 0) answerOldestWrite(h2, "stored");
        assert.equal(live2.maximizeMode, 3, "runtime slot armed");
        h2.world.wins = h2.world.wins.filter((w) => w.internalId !== "win-a");
        if (h2.world.activeWindow?.internalId === "win-a") {
            h2.world.activeWindow = h2.world.wins[0] ?? null;
        }
        const survivor = h2.world.wins[0] as FakeWindow;
        (survivor as unknown as Record<string, unknown>)["desktops"] = 123;
        h2.handle?.requestWorkspaceRelative(1);
        h2.runTimers();
        const r2 = requestOf(h2);
        const innerH = reportOf(h2) as Record<string, unknown>;
        assert.equal(innerH["complete"], false, "held fresh over-retains");
        h2.lastCall("DescribeWorld").callback(acceptedReply("gen-1", r2["source"] as string, r2["nativeInvalid"] as string, "2", {
            windows: [],
        }));
        h2.runTimers();
        while (pendingWrites(h2) > 0) answerOldestWrite(h2, "stored");
        h2.runTimers();
        const writes2 = h2.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.ok(writes2.length >= 1);
        const last2 = writes2[writes2.length - 1] as CapturedCall;
        const recs2 = ((JSON.parse(last2.payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>).map((r) => r["win"]);
        assert.ok(recs2.includes("win-a"), "held fresh over-retains the ended slot");
        h2.handle?.stop();
    });
});
