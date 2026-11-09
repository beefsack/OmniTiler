// Automatic fixed-size float stacking on the shipped world route:
// admission keeps project-owned keepAbove above tiles with zero geometry
// or focus writes, preexisting below/above means zero stacking writes,
// later app/user below wins without writes, closed/replaced refs never
// take cleanup writes, explicit unfloat releases once, focused fullscreen
// games are never covered and take no writes, and a focused automatic
// float keeps native focus across automatic (af-*) reconciles with
// correlated diagnostics. Explicit command-issued (fx-*) focus always
// actuates, even under a floating focus. Fakes use the exact KWin
// property names (keepAbove/keepBelow/fullScreen/maximizeMode/
// onAllDesktops/internalId) and mirror native above/below exclusivity;
// no legacy plan-adapter coverage here (that path is not production).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { startWorldAdapterEntry } from "../src/world-entry";
import type { WorldObservation } from "../src/world-observe";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

// ---------------------------------------------------------------------------
// Shared QML-shaped float fakes with native above/below exclusivity.
// ---------------------------------------------------------------------------

// Array-like inventory exactly like the scripting host: indexed
// properties plus length on a plain object, never a JS Array.
const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

// ---------------------------------------------------------------------------
// World apply focus preservation: a focused automatic float survives an
// automatic reconcile carrying tiled-survivor focus; nothing activates over
// it, with correlated attempt/result diagnostics on the actuated path.
// ---------------------------------------------------------------------------

function applyObservation(focused: string, focusIsFloat: boolean) {
    const floatRef = { id: "win-b" };
    const tileRef = { id: "win-a" };
    const win = (id: string, floating: boolean, fixedAuto: boolean) => ({
        win: id, ref: id === "win-b" ? floatRef : tileRef,
        out: "out", ws: "ws",
        rect: { x: 0, y: 0, w: 100, h: 100 },
        floating, fitExcluded: floating,
        fullscreen: false, maximized: false, sticky: false,
        fixedAuto, fixedSuppress: false, nativeWs: null,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: false,
    });
    return {
        observation: {
            mode: "per-output-local",
            topology: [{ out: "out", order: ["ws"], current: "ws", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
            bindings: [],
            domains: [{
                id: "out", ws: "ws",
                bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 8, outer: 4, focused: focused, readable: true, hold: "",
                closed: [], complete: true,
                windows: [
                    win("win-a", false, false),
                    win("win-b", focusIsFloat, focusIsFloat),
                ],
            }],
            minimized: [],
            predicate: "both-axes-fixed",
            complete: true,
            gesture: false,
            focusedGlobal: focused,
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        } as WorldObservation,
        refs: { floatRef: floatRef as object, tileRef: tileRef as object },
    };
}

function applyBoundReply(target: string, id: string, effect: { cause: "automatic-admission" | "explicit-command"; cmdSeq: string | null }): { state: WorldAdapterState; flightWatermark: string; reply: never } {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation and the local native watermark. Broker
    // obs/rev advance together here; the terminal result stays null
    // (requester-only on the take lane).
    const state = new WorldAdapterState("test-adapter", "owner-a");
    assert.equal(state.onDiscovery("gen-1", "test-adapter"), true);
    state.invalidate();
    const flight = state.beginCapture("req-1", null);
    assert.ok(flight !== null);
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
        obs: "1", rev: "1",
        ready: true, consumed: false,
        source: "req-1",
        nativeInvalid: flight.watermark,
        clock: "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: [], transfers: [],
        effects: [{ id, kind: "focus", target, rev: "1", cause: effect.cause, cmdSeq: effect.cmdSeq, awaitShown: false }],
        result: null,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    if (!isCoreWorldReply(parsed)) {
        throw new Error("unreachable");
    }
    assert.deepEqual(state.acceptReply(parsed, "req-1", flight.watermark), { applied: true, reason: "accepted" });
    return { state, flightWatermark: flight.watermark, reply: parsed as never };
}

describe("world apply automatic-float focus preservation", () => {
    it("a focused automatic float keeps native focus across an automatic af-* reconcile", () => {
        const { observation } = applyObservation("win-b", true);
        // Producer-accurate automatic admission intent (world_engine.rs
        // mints af-<window>); explicit fx-<seq> must never take this path.
        const { state, flightWatermark, reply } = applyBoundReply("win-a", "af-win-a", { cause: "automatic-admission", cmdSeq: null });
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        const result = applyWorldReply({ dispatchWatermark: flightWatermark, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [], "zero activation writes to any window");
        assert.equal(result.effects, 0, "no focus effect counted");
        assert.ok(logs.some((l) => l.includes("focus-skipped") && l.includes("reason=automatic-active") && l.includes("target=win-a")), "automatic-active skip logged");
        assert.ok(!logs.some((l) => l.includes("automatic-focus-attempt")), "protected skip attempts no setter");
    });

    it("an explicit fx-* follow focus actuates under a floating focus", () => {
        // Producer-accurate explicit command outcome (world_report.rs
        // mints fx-<seq> for executed focus commands, e.g. send-follow):
        // user intent must never be swallowed by float preservation.
        const { observation, refs } = applyObservation("win-b", true);
        const { state, flightWatermark, reply } = applyBoundReply("win-a", "fx-3", { cause: "explicit-command", cmdSeq: "1" });
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        // Truthful fake foreground: the native foreground starts on the
        // floating focus and the fake setter moves it to the target.
        let fg: string | null = "win-b";
        const result = applyWorldReply({ dispatchWatermark: flightWatermark, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            readActiveWindowId: () => fg,
            readWindowId: (ref) => ref === refs.tileRef ? "win-a" : ref === refs.floatRef ? "win-b" : null,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); if (ref === refs.tileRef) { fg = "win-a"; } return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.tileRef], "explicit follow actuates despite the floating focus");
        assert.equal(result.effects, 1);
        assert.ok(!logs.some((l) => l.includes("focus-skipped")), "no preservation skip for explicit focus");
    });

    it("a tiled focus target still actuates with correlated attempt/result diagnostics", () => {
        const { observation, refs } = applyObservation("win-a", false);
        const { state, flightWatermark, reply } = applyBoundReply("win-b", "fx-7", { cause: "explicit-command", cmdSeq: "1" });
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        // Truthful fake foreground: starts on win-a, the fake setter moves
        // it to the win-b target.
        let fg: string | null = "win-a";
        const result = applyWorldReply({ dispatchWatermark: flightWatermark, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            readActiveWindowId: () => fg,
            readWindowId: (ref) => ref === refs.tileRef ? "win-a" : ref === refs.floatRef ? "win-b" : null,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); if (ref === refs.floatRef) { fg = "win-b"; } return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.floatRef], "actuates the planned target");
        assert.equal(result.effects, 1);
        assert.ok(logs.some((l) => l.includes("automatic-focus-attempt") && l.includes("target=win-b")), "attempt logged");
        assert.ok(logs.some((l) => l.includes("automatic-focus-result") && l.includes("target=win-b") && l.includes("outcome=setter-accepted proven=true")), "acceptance logged");
    });

    it("an accepted setter without native activation reports proven=false, not refused", () => {
        const { observation, refs } = applyObservation("win-a", false);
        const { state, flightWatermark, reply } = applyBoundReply("win-b", "fx-7", { cause: "explicit-command", cmdSeq: "1" });
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        // Truthful fake foreground: the setter accepts but the native
        // foreground never arrives, so proof fails closed.
        const fg: string | null = "win-a";
        const result = applyWorldReply({ dispatchWatermark: flightWatermark, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            // Setter accepts, but native activation never proves: the
            // outcome must stay setter-accepted with a separate false
            // proven token instead of degrading to setter-refused.
            readActiveWindowId: () => fg,
            readWindowId: (ref) => ref === refs.tileRef ? "win-a" : ref === refs.floatRef ? "win-b" : null,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.floatRef], "setter ran once");
        assert.equal(result.effects, 0, "unproven activation counts nothing");
        assert.ok(logs.some((l) => l.includes("automatic-focus-result") && l.includes("outcome=setter-accepted proven=false")), "accepted without activation");
        assert.ok(!logs.some((l) => l.includes("outcome=setter-refused")), "never misreported as refused");
    });
});

// ---------------------------------------------------------------------------
// Production entry: automatic keepAbove exactly once with zero frame, focus
// or keepBelow writes; owned lifecycle across tile/unfloat, replacement,
// restart and fullscreen legs.
// ---------------------------------------------------------------------------

interface EntryDesk { id: string; name: string }
interface EntryWin {
    internalId: string;
    normalWindow: boolean;
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    // Truthful undecorated client rect: the fake carries no decoration, so
    // the client mirrors the frame (delta 0).
    clientGeometry: { x: number; y: number; width: number; height: number } | null;
    output: { name: string };
    desktops: EntryDesk[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    setMaximize: (v: boolean, h: boolean) => void;
}

interface EntryHarness {
    wins: EntryWin[];
    world: {
        screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
        desktops: EntryDesk[];
        activeScreen: { name: string };
        activeWindow: EntryWin | null;
        currentDesktop: EntryDesk;
        focused: string[];
        geometryWrites: string[];
    };
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    aboveWrites: string[];
    belowWrites: string[];
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: (maxDelay?: number) => void;
    rev: { n: number };
}

function entryWin(id: string, out: string, ws: EntryDesk, fixed: boolean, above: string[], below: string[]): EntryWin {
    let keepAbove = false;
    let keepBelow = false;
    const w = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: fixed ? 640 : 200, height: fixed ? 480 : 200 },
        // Truthful undecorated client rect mirrors the frame exactly.
        clientGeometry: { x: 0, y: 0, width: fixed ? 640 : 200, height: fixed ? 480 : 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: fixed ? { width: 640, height: 480 } : null,
        maxSize: fixed ? { width: 640, height: 480 } : null,
        setMaximize: (v: boolean, h: boolean): void => { w.maximizeMode = v && h ? 3 : 0; },
    } as EntryWin;
    Object.defineProperty(w, "keepAbove", {
        configurable: true, enumerable: true,
        get: () => keepAbove,
        set: (v: unknown) => {
            above.push(`${id}=${String(v)}`);
            keepAbove = v === true;
            if (v === true) {
                keepBelow = false;
            }
        },
    });
    Object.defineProperty(w, "keepBelow", {
        configurable: true, enumerable: true,
        get: () => keepBelow,
        set: (v: unknown) => {
            below.push(`${id}=${String(v)}`);
            keepBelow = v === true;
            if (v === true) {
                keepAbove = false;
            }
        },
    });
    return w;
}

function makeEntryHarness(qmlLists = false): EntryHarness {
    const ws1: EntryDesk = { id: "ws1", name: "ws1" };
    const ws2: EntryDesk = { id: "ws2", name: "ws2" };
    const aboveWrites: string[] = [];
    const belowWrites: string[] = [];
    const wins: EntryWin[] = [
        entryWin("win-a", "out", ws1, false, aboveWrites, belowWrites),
        entryWin("win-b", "out", ws1, true, aboveWrites, belowWrites),
    ];
    const world = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        activeScreen: { name: "out" },
        activeWindow: wins[0] as EntryWin | null,
        currentDesktop: ws1,
        focused: [] as string[],
        geometryWrites: [] as string[],
    };
    const calls: EntryHarness["calls"] = [];
    const timers: EntryHarness["timers"] = [];
    const logs: string[] = [];
    const surface = {
        windowList: () => qmlLists ? qmlLike(wins) : wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            const found = wins.find((w) => (w as unknown) === ref);
            if (found !== undefined) {
                world.focused.push(found.internalId);
            }
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            const found = world.desktops.find((d) => (d as unknown) === ref);
            if (found !== undefined) {
                world.currentDesktop = found;
            }
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => undefined,
        removeDesktop: () => false,
    };
    for (const w of wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                // Keep the truthful undecorated client tracking the frame so
                // no capture pairs a fresh frame with a stale client.
                (w as unknown as Record<string, unknown>)["clientGeometry"] = { ...rect };
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
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
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"],
                    globalCmd: String(calls.filter((q) => q.method === "QueueWorldCommand").length) }));
            } else if (method === "TakeCommandResult") {
                const takeBody = JSON.parse(payload) as Record<string, unknown>;
                const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
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
        log: (message) => { logs.push(message); },
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
    return { wins, world, calls, timers, logs, aboveWrites, belowWrites, handle, runTimers, rev: { n: 0 } };
}

function answerHandshake(h: { calls: EntryHarness["calls"] }, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse((found as EntryHarness["calls"][number]).payload) as Record<string, unknown>)["req"] as string;
    (found as EntryHarness["calls"][number]).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: { calls: EntryHarness["calls"] }, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse((found as EntryHarness["calls"][number]).payload) as Record<string, unknown>;
    (found as EntryHarness["calls"][number]).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function entryAcceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation and native watermark. This scripted
    // harness advances broker obs and revision together; terminal results
    // stay requester-only on the take lane.
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function entryDiscover(h: EntryHarness, gen = "gen-1"): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    // Settle the discovery-triggered observation report first: the entry
    // auto-dispatches one observation-only DescribeWorld on bind, and a
    // command issued while it is in flight queues in FIFO order behind it.
    // Replying here leaves the flight idle for the test body.
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined);
    const autoBody = JSON.parse(auto.payload);
    assert.equal(autoBody["command"], undefined, "captures never carry an inline command");
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(entryAcceptedReply(gen, autoBody["source"] as string, autoBody["nativeInvalid"] as string, "0"));
}

function entryRound(h: EntryHarness, extra: Record<string, unknown> = {}): void {
    h.handle?.requestWorkspaceRelative(1);
    h.runTimers();
    const call = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(call !== undefined);
    const body = JSON.parse(call.payload);
    h.rev.n += 1;
    // Terminal results ride the take lane requester-only, never the
    // capture reply.
    const { result, ...snapshot } = extra;
    if (result !== undefined) {
        (globalThis as Record<string, unknown>)["__takeResult"] = result;
    }
    call.callback(entryAcceptedReply("gen-1", body.source, body.nativeInvalid, String(h.rev.n), snapshot));
}

describe("world entry automatic-float stacking", () => {
    it("admits the automatic float with zero stacking writes", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        // Automatic keepAbove stacking is retired on the production
        // entry (helper removed): admission writes neither above nor below.
        assert.deepEqual(h.aboveWrites, [], "admission writes no keepAbove");
        assert.deepEqual(h.belowWrites, [], "zero keepBelow writes");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@")), "zero frame writes to the float");
        assert.deepEqual(h.world.focused, [], "zero focus writes");
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "retained observation adds no stacking writes");
        assert.deepEqual(h.belowWrites, [], "still zero below writes");
        assert.ok(!h.logs.some((l) => l.includes("automatic-keep-above")), "no automatic stacking actuation");
    });

    it("a focused automatic float survives a survivor-focus reconcile with no activation", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        h.world.activeWindow = h.wins.find((w) => w.internalId === "win-b") ?? null;
        entryRound(h, {
            effects: [{ id: "af-win-a", kind: "focus", target: "win-a", rev: "9", cause: "automatic-admission", cmdSeq: null, awaitShown: false }],
        });
        assert.deepEqual(h.world.focused, [], "no activation over the focused float");
        assert.ok(h.logs.some((l) => l.includes("focus-skipped") && l.includes("reason=automatic-active")), "automatic-active skip");
    });

    it("an explicit follow focus actuates under a focused automatic float", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        h.world.activeWindow = h.wins.find((w) => w.internalId === "win-b") ?? null;
        entryRound(h, {
            effects: [{ id: "fx-3", kind: "focus", target: "win-a", rev: "9", cause: "explicit-command", cmdSeq: "1", awaitShown: false }],
        });
        assert.deepEqual(h.world.focused, ["win-a"], "explicit fx-* follow actuates");
        assert.ok(!h.logs.some((l) => l.includes("focus-skipped")), "no preservation skip for explicit focus");
    });

    it("explicit unfloat tiles with zero stacking writes and no below write", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "admission writes nothing");
        entryRound(h, {
            // Strict v4 toggle-float success is the exact 3-key shape
            // (world-report.ts result-float-keys): present-but-false
            // noSpace refuses, so success omits the key entirely.
            result: { op: "toggle-float", win: "win-b", rect: null },
        });
        // The explicit-unfloat release is a no-op with nothing owned
        // (retired automatic stacking): the tile proceeds with zero
        // stacking writes and never a keepBelow write.
        assert.deepEqual(h.aboveWrites, [], "explicit tile writes no above");
        assert.deepEqual(h.belowWrites, [], "tile never writes below");
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "no repeat writes");
        assert.deepEqual(h.belowWrites, [], "still zero below writes");
    });

    it("a replaced native reference under a reused id stays silent with no stale writes", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "admission writes nothing");
        // Close gap: the float leaves the inventory for one accepted round.
        const closed = h.wins.find((w) => w.internalId === "win-b");
        assert.ok(closed !== undefined);
        h.wins.splice(h.wins.indexOf(closed), 1);
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "closed ref takes no cleanup write");
        // A fresh native reference arrives under the reused id.
        const ws1 = h.world.desktops[0] as EntryDesk;
        const fresh = entryWin("win-b", "out", ws1, true, h.aboveWrites, h.belowWrites);
        h.wins.push(fresh);
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "the fresh ref acquires nothing");
        assert.equal((fresh as unknown as Record<string, unknown>)["keepAbove"], false);
    });

    it("restart after explicit stop stays silent with zero stacking writes", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "admission writes nothing");
        // Explicit stop releases nothing (nothing is owned under retired
        // automatic stacking); a restart over the same live clients
        // re-admits with zero writes as well.
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, [], "stop writes nothing");
        const h2 = makeEntryHarness();
        h2.wins.splice(0, h2.wins.length, ...h.wins);
        h2.world.activeWindow = h.wins[0] ?? null;
        entryDiscover(h2);
        assert.deepEqual(h.aboveWrites, [], "restart re-admits with zero writes");
        assert.deepEqual(h2.aboveWrites, [], "restart instance wrote nothing itself");
        assert.deepEqual(h2.belowWrites, [], "zero below writes");
        entryRound(h2);
        assert.deepEqual(h.aboveWrites, [], "retained adds nothing");
        h2.handle?.stop();
    });

    it("a focused fullscreen game takes zero stacking writes on suspend and exit", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "admission writes nothing");
        // With nothing owned under retired automatic stacking, a focused
        // fullscreen game suspends nothing and re-acquires nothing: zero
        // writes to the game and to the float on both transitions.
        const game = h.wins.find((w) => w.internalId === "win-a") as EntryWin;
        game.fullScreen = true;
        h.world.activeWindow = game;
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "game focus suspends nothing");
        assert.deepEqual(h.belowWrites, [], "no below writes");
        assert.deepEqual(h.world.focused, [], "no focus writes to the game");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-a@")), "no frame writes to the game");
        // Game exit re-acquires nothing.
        game.fullScreen = false;
        entryRound(h);
        assert.deepEqual(h.aboveWrites, [], "exit acquires nothing");
    });

    it("explicit stop writes nothing (handle.stop only, not KWin unload)", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "admission writes nothing");
        // handle.stop() is the only harnessed teardown: the actual KWin
        // script unload path is unharnessed and makes no claim here.
        // With nothing owned under retired automatic stacking, stop
        // releases nothing.
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, [], "stop releases nothing");
        assert.deepEqual(h.belowWrites, [], "stop never writes below");
    });

    it("explicit stop writes nothing for a live-fullscreen float", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        const owned = h.wins.find((w) => w.internalId === "win-b") as EntryWin;
        owned.fullScreen = true;
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, [], "zero writes to the fullscreen client on stop");
        assert.deepEqual(h.belowWrites, [], "zero below writes");
    });


    it("admits tiles with zero stacking writes through QML-shaped array-like inventories", () => {
        const h = makeEntryHarness(true);
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, [], "array-like windowList still converges with zero writes");
        assert.deepEqual(h.belowWrites, [], "zero keepBelow writes");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@")), "zero frame writes to the float");
        assert.deepEqual(h.world.focused, [], "zero focus writes");
        h.handle?.stop();
    });
});
