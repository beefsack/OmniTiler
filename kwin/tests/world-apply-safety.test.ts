// World overlay safety: native KWin predicates (`fullscreen`,
// `maximized`, `sticky`, `minimized` plus the hidden-into-minimized alias)
// classify overlays exactly like the legacy plan sources; a
// fullscreen/minimized/hidden flip between observation and actuation fails
// closed; automatic focus/raise/stack never touches game/excluded/overlay
// rows.
//
// STRICT V4 SYNTHETIC replies (hand-authored inline for mechanical apply
// only and validated below through the real parseWorldReply; these bytes
// are not real-core output): the handshake/settings bootstrap binds the
// producer adapter lifetime, captures echo source correlation with an
// independent broker obs, and the terminal result rides TakeCommandResult
// requester-only.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { captureWorldObservation } from "../src/world-observe";
import { applyWorldReply } from "../src/world-apply";
import { createWorldApplyHost } from "../src/world-native";
import { WorldAdapterState } from "../src/world-state";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

function makeSurface() {
    const ws1 = { id: "ws-1" };
    const ws2 = { id: "ws-2" };
    const wins = [
        {
            internalId: "win-tiled", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-game", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 1920, height: 1080 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: true, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-sticky", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: true,
            minSize: null, maxSize: null,
        },
    ];
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as unknown,
        currentDesktop: ws1 as { id: string },
        geometryWrites: [] as string[],
    };
    for (const w of wins) {
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
    }
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
            world.activeWindow = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as { id: string };
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

function answerHandshake(calls: Call[], gen = "gen-1"): void {
    const found = [...calls].reverse().find((c) => {
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
    const req = (JSON.parse((found as Call).payload) as Record<string, unknown>)["req"] as string;
    (found as Call).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(calls: Call[], gen = "gen-1"): void {
    const found = [...calls].reverse().find((c) => {
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
    const body = JSON.parse((found as Call).payload) as Record<string, unknown>;
    (found as Call).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function runTimers(timers: Array<{ delay: number; cb: () => void }>): void {
    for (let i = 0; i < timers.length; i += 1) {
        const t = timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
}

// Terminal take collection: pending while no command drained (the capture
// reply result lane stays null, requester-only via take).
function answerTake(calls: Call[]): void {
    const take = [...calls].reverse().find((c) => c.method === "TakeCommandResult");
    if (take === undefined) {
        return;
    }
    const takeBody = JSON.parse((take as Call).payload) as Record<string, unknown>;
    const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
    (take as Call).callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
}

function fusedWs(id: string, order: number): Record<string, unknown> {
    return { id, out: "out-1", bind: id, scope: { kind: "output", out: "out-1" }, order, mode: "tiled", exist: true, hold: "" };
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation, the native watermark, and an
    // independent broker obs; the terminal result stays null (the take
    // lane serves it requester-only).
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        closeFocus: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out-1", ws: "ws-1" }, auth: "controlled", reason: "" }],
        workspaces: [fusedWs("ws-1", 0), fusedWs("ws-2", 1)],
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

// Genuine 18-key v4 window row (retired 13-key shape refuses).
function winRow(winId: string, ws: string, rect: [number, number, number, number]): Record<string, unknown> {
    return {
        win: winId, out: "out-1", ws,
        alloc: [...rect], frame: [...rect],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "", clear_op: "",
        cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "",
        state: [],
    };
}

// Automatic focus effect (exact 7-key shape; explicit-command would demand
// a correlated command that these legs never file).
function focusFx(id: string, target: string): Record<string, unknown> {
    return { id, kind: "focus", target, rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false };
}

// Stable core window-transfer identity the membership legs ride under (v4
// authorizes no move without one).
function moveTransfer(): Record<string, unknown> {
    return { op: "op-1", kind: "window", win: "win-tiled", out: "out-1", ws: "ws-2", follow: false, held: false, cmdSeq: null };
}

function start(surface: object) {
    const calls: Call[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
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
                callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(calls);
    answerSettings(calls);
    runTimers(timers);
    const auto = calls.filter((c) => c.method === "DescribeWorld").pop();
    assert.ok(auto !== undefined, "expected DescribeWorld after settings accept");
    const autoBody = JSON.parse((auto as Call).payload) as Record<string, unknown>;
    assert.equal(autoBody["command"], undefined, "captures never carry an inline command");
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    return { handle: handle as NonNullable<typeof handle>, calls, timers, auto: auto as Call };
}

describe("world overlay safety", () => {
    it("captures native fullscreen/maximized/sticky truthfully for broker classification", () => {
        const { surface, world } = makeSurface();
        const { handle, auto } = start(surface);
        const envelope = JSON.parse(auto.payload) as Record<string, unknown>;
        const inner = envelope["report"] as Record<string, unknown>;
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        const game = wins.find((w) => w["win"] === "win-game");
        assert.ok(game !== undefined, "fullscreen game window is observed");
        assert.equal(game["fullscreen"], true);
        assert.equal(game["maximized"], false);
        assert.equal(game["sticky"], false);
        assert.equal(game["minimized"], false);
        assert.deepEqual(game["rect"], [0, 0, 1920, 1080]);
        assert.deepEqual(game["hints"], { minW: null, minH: null, maxW: null, maxH: null });
        assert.equal(game["nativeWs"], "ws-1");
        // No projected verdicts ride the native lane: the broker derives
        // fitExcluded/floating from these native facts via the shared
        // predicates (pinned by the Rust raw tests).
        assert.ok(!("fitExcluded" in game) && !("floating" in game), "no wire verdicts");
        const sticky = wins.find((w) => w["win"] === "win-sticky");
        assert.ok(sticky !== undefined);
        assert.equal(sticky["sticky"], true);
        assert.equal(sticky["nativeWs"], "ws-2");
        assert.ok(!("floating" in sticky), "no wire verdicts");
        const tiled = wins.find((w) => w["win"] === "win-tiled");
        assert.ok(tiled !== undefined);
        assert.equal(tiled["fullscreen"], false);
        assert.equal(tiled["maximized"], false);
        assert.equal(tiled["sticky"], false);
        assert.equal(tiled["nativeWs"], "ws-1");
        assert.ok(!("fitExcluded" in tiled), "no wire verdicts");
        assert.equal(world.geometryWrites.length, 0);
        handle.stop();
    });

    it("refuses geometry when fullscreen flips between observation and actuation", () => {
        const { surface, world } = makeSurface();
        const { handle, calls, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        // Native flip after the observation was captured, before the reply
        // applies: the setter must fail closed.
        (world.wins[0] as { fullScreen: boolean }).fullScreen = true;
        auto.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            windows: [winRow("win-tiled", "ws-1", [10, 10, 300, 300])],
        }));
        answerTake(calls);
        assert.equal(world.geometryWrites.length, 0, "flipped overlay takes no frame write");
        handle.stop();
    });

    it("refuses membership when minimized flips between observation and actuation", () => {
        // Stale-observation direct apply: the entry recaptures fresh before
        // every apply, so this flip veto is only observable against the
        // pre-flip capture (fresh would already carry minimized:true and
        // the gate could not distinguish the flip).
        const { surface, world } = makeSurface();
        const stale = captureWorldObservation({
            workspace: surface as never,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: () => null,
        });
        assert.ok(stale !== null);
        (world.wins[0] as { minimized: boolean }).minimized = true;
        const readProp = (v: object, p: string): unknown => Reflect.get(v as object, p);
        const host = createWorldApplyHost(surface as object, readProp as never);
        // Genuine v4 bind on the SAME state: the captured flight names the
        // reply source, the native watermark rides truthfully, and the
        // broker obs stays independent (never overwritten with the fence).
        const state = new WorldAdapterState("test-adapter", "owner-a");
        assert.equal(state.onDiscovery("gen-1", "test-adapter", "disc-1", "disc-1"), true);
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const text = JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: "1", rev: "1", ready: true, consumed: false,
            source: "req-1", nativeInvalid: flight.watermark, clock: "1",
            gestureSettled: null, gestureAssoc: null,
            closeFocus: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [], workspaces: [fusedWs("ws-1", 0), fusedWs("ws-2", 1)], outputs: [],
            windows: [winRow("win-tiled", "ws-2", [10, 10, 300, 300])],
            transfers: [moveTransfer()], effects: [], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        assert.deepEqual(state.acceptReply(parsed, "req-1", flight.watermark), { applied: true, reason: "accepted" });
        const result = applyWorldReply({
            state,
            observation: stale,
            reply: parsed,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            log: () => undefined,
            ...host,
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: () => true,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.equal(result.stoppedMidApply, true, "stale minimized flip vetoes at the pre-setter gate");
        assert.equal(result.membershipWrites, 0, "vetoed move writes no membership");
        assert.equal((world.wins[0] as { desktops: Array<{ id: string }> }).desktops[0]?.id, "ws-1", "minimized flip blocks the move");
    });

    it("moves an already-minimized window under the same membership desire", () => {
        // Control: the same desire against a freshly observed minimized
        // baseline is a legitimate same-output relocation (still minimized,
        // membership only), proving the leg above vetoes the flip rather
        // than banning minimized membership itself.
        const { surface, world } = makeSurface();
        (world.wins[0] as { minimized: boolean }).minimized = true;
        const fresh = captureWorldObservation({
            workspace: surface as never,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: () => null,
        });
        assert.ok(fresh !== null);
        const readProp = (v: object, p: string): unknown => Reflect.get(v as object, p);
        const host = createWorldApplyHost(surface as object, readProp as never);
        const state = new WorldAdapterState("test-adapter", "owner-a");
        assert.equal(state.onDiscovery("gen-1", "test-adapter", "disc-1", "disc-1"), true);
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const text = JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: "1", rev: "1", ready: true, consumed: false,
            source: "req-1", nativeInvalid: flight.watermark, clock: "1",
            gestureSettled: null, gestureAssoc: null,
            closeFocus: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [], workspaces: [fusedWs("ws-1", 0), fusedWs("ws-2", 1)], outputs: [],
            windows: [winRow("win-tiled", "ws-2", [10, 10, 300, 300])],
            transfers: [moveTransfer()], effects: [], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        assert.deepEqual(state.acceptReply(parsed, "req-1", flight.watermark), { applied: true, reason: "accepted" });
        const result = applyWorldReply({
            state,
            observation: fresh,
            reply: parsed,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            log: () => undefined,
            ...host,
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: () => true,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.equal((world.wins[0] as { desktops: Array<{ id: string }> }).desktops[0]?.id, "ws-2", "observed minimized relocation actuates");
        assert.equal((world.wins[0] as { minimized: boolean }).minimized, true, "relocation keeps minimized");
        assert.equal(world.geometryWrites.length, 0, "minimized relocation writes no frame");
        assert.equal(result.membershipWrites, 1, "one membership write actuates the relocation");
    });

    it("refuses geometry when hidden flips between observation and actuation", () => {
        // Stale-observation direct apply with the real native driver: the
        // observation is captured before the flip, so the live safety alias
        // (hidden into minimized) fences the setter.
        const { surface, world } = makeSurface();
        const stale = captureWorldObservation({
            workspace: surface as never,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: () => null,
        });
        assert.ok(stale !== null);
        (world.wins[0] as { hidden: boolean }).hidden = true;
        const readProp = (v: object, p: string): unknown => Reflect.get(v as object, p);
        const host = createWorldApplyHost(surface as object, readProp as never);
        // Genuine v4 bind on the SAME state: the captured flight names the
        // reply source, the native watermark rides truthfully, and the
        // broker obs stays independent (never overwritten with the fence).
        const state = new WorldAdapterState("test-adapter", "owner-a");
        assert.equal(state.onDiscovery("gen-1", "test-adapter", "disc-1", "disc-1"), true);
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const text = JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: "1", rev: "1", ready: true, consumed: false,
            source: "req-1", nativeInvalid: flight.watermark, clock: "1",
            gestureSettled: null, gestureAssoc: null,
            closeFocus: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [], workspaces: [], outputs: [],
            windows: [winRow("win-tiled", "ws-1", [10, 10, 300, 300])],
            transfers: [], effects: [], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        assert.deepEqual(state.acceptReply(parsed, "req-1", flight.watermark), { applied: true, reason: "accepted" });
        const result = applyWorldReply({
            state,
            observation: stale,
            reply: parsed,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            log: () => undefined,
            ...host,
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: () => true,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.equal(world.geometryWrites.length, 0, "hidden flip takes no frame write");
        assert.equal(result.geometryWrites, 0);
    });

    it("refuses focus when hidden flips before the focus effect", () => {
        const { surface, world } = makeSurface();
        const stale = captureWorldObservation({
            workspace: surface as never,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: () => null,
        });
        assert.ok(stale !== null);
        const before = world.activeWindow;
        (world.wins[0] as { hidden: boolean }).hidden = true;
        const readProp = (v: object, p: string): unknown => Reflect.get(v as object, p);
        const host = createWorldApplyHost(surface as object, readProp as never);
        // Same genuine bind as above: the refusal below must come from the
        // hidden flip at the focus gate, never from a stale or malformed
        // reply.
        const state = new WorldAdapterState("test-adapter", "owner-a");
        assert.equal(state.onDiscovery("gen-1", "test-adapter", "disc-1", "disc-1"), true);
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const text = JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: "1", rev: "1", ready: true, consumed: false,
            source: "req-1", nativeInvalid: flight.watermark, clock: "1",
            gestureSettled: null, gestureAssoc: null,
            closeFocus: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [], workspaces: [], outputs: [],
            windows: [],
            transfers: [], effects: [focusFx("fx-9", "win-tiled")], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        assert.deepEqual(state.acceptReply(parsed, "req-1", flight.watermark), { applied: true, reason: "accepted" });
        const focusWrites: unknown[] = [];
        applyWorldReply({
            state,
            observation: stale,
            reply: parsed,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            log: () => undefined,
            ...host,
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
        assert.deepEqual(focusWrites, [], "hidden flip holds the focus effect");
        assert.equal(world.activeWindow, before);
    });

    it("never steers automatic focus onto a game/excluded row", () => {
        const { surface, world } = makeSurface();
        const { handle, calls, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        const before = world.activeWindow;
        auto.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            effects: [focusFx("fx-1", "win-game")],
        }));
        answerTake(calls);
        assert.equal(world.activeWindow, before, "overlay focus effect held, never stolen");
        handle.stop();
    });

    it("still applies automatic focus onto a tiled row", () => {
        const { surface, world } = makeSurface();
        world.activeWindow = world.wins[1] as unknown;
        const { handle, calls, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        auto.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1", {
            effects: [focusFx("fx-2", "win-tiled")],
        }));
        answerTake(calls);
        assert.equal(
            ((world.activeWindow as unknown as { internalId: string }).internalId),
            "win-tiled",
            "tiled focus effect applies",
        );
        handle.stop();
    });
});
