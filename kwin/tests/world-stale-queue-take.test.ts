// Stale queue/take lane regression (strict v4, sole broker): a delayed
// old-generation QueueWorldCommand/TakeCommandResult reply after a
// lifetime rotation must no-op. It never files, never drains, never clears
// newer undecided state, and never applies stale actuation or terminal
// results. Offline only: scripted in-memory transport, no D-Bus, no KWin.
//
// STRICT V4 SYNTHETIC replies (hand-authored inline, never real-core
// output): the handshake/settings bootstrap binds the producer adapter
// lifetime, captures echo source correlation with an independent broker
// obs, and the terminal result rides TakeCommandResult requester-only.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function windowRow(frame: [number, number, number, number]): Record<string, unknown> {
    return {
        win: "win-a", out: "out", ws: "ws1",
        alloc: [...frame], frame: [...frame],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "",
        cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "",
        state: [],
    };
}

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
    ];
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0] as unknown,
        currentDesktop: ws1,
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
        sendClientToScreen: () => undefined,
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

function startFlow() {
    const { surface, world } = makeSurface();
    const calls: Call[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
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
        log: (message: string) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, world, logs };
}

function lastCall(calls: Call[], method: string): Call {
    const call = [...calls].reverse().find((c) => c.method === method);
    assert.ok(call !== undefined, `expected ${method}`);
    return call as Call;
}

function runTimers(flow: ReturnType<typeof startFlow>): void {
    for (let i = 0; i < flow.timers.length; i += 1) {
        const t = flow.timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            flow.timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
}

function answerHandshake(flow: ReturnType<typeof startFlow>, gen: string): void {
    const disc = [...flow.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(disc !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse(disc.payload) as Record<string, unknown>)["req"] as string;
    disc.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(flow: ReturnType<typeof startFlow>, gen: string): void {
    const found = [...flow.calls].reverse().find((c) => {
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
    const body = JSON.parse(found.payload) as Record<string, unknown>;
    found.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function acceptCapture(
    gen: string,
    envelope: Record<string, unknown>,
    rev: string,
    frame: [number, number, number, number] = [10, 10, 300, 300],
): string {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation and native watermark. Scripted broker obs
    // advances with rev; the terminal result stays null (requester-only
    // via take).
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: envelope["source"], obs: rev, rev,
        ready: true, consumed: false,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: [windowRow(frame)],
        transfers: [], effects: [], result: null,
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function generationMismatch(gen: string): string {
    return JSON.stringify({
        v: 4, outcome: "rejected", kind: "generation-mismatch",
        message: "generation mismatch", gen, source: "", nativeInvalid: "0", clock: "1",
        gestureAssoc: null,
    });
}

function discoverFlow(flow: ReturnType<typeof startFlow>, gen: string): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies. The caller settles the auto capture.
    answerHandshake(flow, gen);
    answerSettings(flow, gen);
    runTimers(flow);
}

function queueOk(flow: ReturnType<typeof startFlow>, gen: string, globalCmd: string): Call {
    const queue = lastCall(flow.calls, "QueueWorldCommand");
    const filed = JSON.parse(queue.payload) as Record<string, unknown>;
    assert.equal(filed["gen"], gen, "queue binds the serving generation");
    queue.callback(JSON.stringify({
        v: 4, outcome: "queued", gen,
        client: filed["client"], clientCmd: filed["clientCmd"], globalCmd,
    }));
    return queue;
}

function acceptDrain(flow: ReturnType<typeof startFlow>, gen: string, rev: string): Call {
    const drained = lastCall(flow.calls, "DescribeWorld");
    const body = JSON.parse(drained.payload) as Record<string, unknown>;
    assert.equal(body["gen"], gen, "drain binds the serving generation");
    assert.equal(body["command"], undefined, "captures never carry an inline command");
    drained.callback(acceptCapture(gen, body, rev));
    return drained;
}

function takeReady(flow: ReturnType<typeof startFlow>, gen: string, globalCmd: string): Call {
    const take = lastCall(flow.calls, "TakeCommandResult");
    const body = JSON.parse(take.payload) as Record<string, unknown>;
    assert.equal(body["gen"], gen, "take binds the serving generation");
    take.callback(JSON.stringify({ v: 4, outcome: "ready", gen, globalCmd, result: null }));
    return take;
}

describe("stale queue/take lanes after a lifetime rotation", () => {
    it("a delayed old-generation queue reply never files, drains, or clears newer state", () => {
        const flow = startFlow();
        discoverFlow(flow, "gen-1");
        const auto = lastCall(flow.calls, "DescribeWorld");
        auto.callback(acceptCapture("gen-1", JSON.parse(auto.payload) as Record<string, unknown>, "1"));
        // Two commands file while both queue replies are held: Q1 stays
        // held past the rotation below.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        const q1 = lastCall(flow.calls, "QueueWorldCommand");
        assert.equal((JSON.parse(q1.payload) as Record<string, unknown>)["gen"], "gen-1");
        const q1client = (JSON.parse(q1.payload) as Record<string, unknown>)["clientCmd"];
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        const q2 = lastCall(flow.calls, "QueueWorldCommand");
        assert.notEqual(q2, q1);
        // Q2 races the rotation: the serving world is already gen-2.
        q2.callback(generationMismatch("gen-2"));
        // The dead generation drops and the entry re-discovers.
        const rediscover = lastCall(flow.calls, "DiscoverWorld");
        assert.notEqual(rediscover, undefined);
        discoverFlow(flow, "gen-2");
        const auto2 = lastCall(flow.calls, "DescribeWorld");
        const auto2body = JSON.parse(auto2.payload) as Record<string, unknown>;
        assert.equal(auto2body["gen"], "gen-2", "post-rotation capture binds the new generation");
        auto2.callback(acceptCapture("gen-2", auto2body, "1"));
        // A fresh command files under the new generation and settles.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        const q3 = lastCall(flow.calls, "QueueWorldCommand");
        const q3body = JSON.parse(q3.payload) as Record<string, unknown>;
        assert.equal(q3body["gen"], "gen-2");
        assert.equal(q3body["clientCmd"], "1", "queue-client namespace restarts on rotation");
        queueOk(flow, "gen-2", "1");
        acceptDrain(flow, "gen-2", "2");
        takeReady(flow, "gen-2", "1");
        const describes = flow.calls.filter((c) => c.method === "DescribeWorld").length;
        const writes = flow.world.geometryWrites.length;
        // The delayed Q1 success (gen-1) arrives after the reset: no-op.
        // It never drains, never clears newer undecided state, never wedges.
        q1.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: (JSON.parse(q1.payload) as Record<string, unknown>)["client"],
            clientCmd: q1client, globalCmd: "9",
        }));
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length,
            describes,
            "stale queue reply drains nothing",
        );
        assert.equal(flow.world.geometryWrites.length, writes, "stale queue reply applies nothing");
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:queue-stale")),
            "stale queue reply logs its no-op",
        );
        // The stream continues: the next command files under the new
        // generation with the next client id.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        const q4 = lastCall(flow.calls, "QueueWorldCommand");
        const q4body = JSON.parse(q4.payload) as Record<string, unknown>;
        assert.equal(q4body["gen"], "gen-2");
        assert.equal(q4body["clientCmd"], "2", "newer undecided state survived the stale reply");
        flow.handle.stop();
    });

    it("a delayed old-generation take reply never applies stale actuation", () => {
        const flow = startFlow();
        discoverFlow(flow, "gen-1");
        const auto = lastCall(flow.calls, "DescribeWorld");
        auto.callback(acceptCapture("gen-1", JSON.parse(auto.payload) as Record<string, unknown>, "1"));
        // First command drains and its take stays held past the rotation.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        queueOk(flow, "gen-1", "1");
        const drain1 = lastCall(flow.calls, "DescribeWorld");
        const drain1body = JSON.parse(drain1.payload) as Record<string, unknown>;
        drain1.callback(acceptCapture("gen-1", drain1body, "2"));
        const t1 = lastCall(flow.calls, "TakeCommandResult");
        // Second command drains; its take races the rotation.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        queueOk(flow, "gen-1", "2");
        acceptDrain(flow, "gen-1", "3");
        const t2 = lastCall(flow.calls, "TakeCommandResult");
        t2.callback(generationMismatch("gen-2"));
        discoverFlow(flow, "gen-2");
        // The rotated world settles a distinct frame so any stale apply is
        // visible as a geometry write.
        const auto2 = lastCall(flow.calls, "DescribeWorld");
        const auto2body = JSON.parse(auto2.payload) as Record<string, unknown>;
        auto2.callback(acceptCapture("gen-2", auto2body, "1", [50, 50, 300, 300]));
        const writes = flow.world.geometryWrites.length;
        assert.ok(writes > 0, "rotated world actuated its own frame");
        const describes = flow.calls.filter((c) => c.method === "DescribeWorld").length;
        const applied = flow.logs.filter((l) => l.startsWith("omnitiler:world:applied")).length;
        // The delayed T1 terminal result (gen-1) arrives after the reset:
        // it never applies stale actuation and never wedges the stream.
        t1.callback(JSON.stringify({ v: 4, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }));
        assert.equal(flow.world.geometryWrites.length, writes, "stale take reply applies nothing");
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length,
            describes,
            "stale take reply dispatches nothing",
        );
        assert.equal(
            flow.logs.filter((l) => l.startsWith("omnitiler:world:applied")).length,
            applied,
            "stale take reply runs no post-accept apply",
        );
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:take-stale")),
            "stale take reply logs its no-op",
        );
        // The stream continues under the new generation.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers(flow);
        const q3 = lastCall(flow.calls, "QueueWorldCommand");
        assert.equal((JSON.parse(q3.payload) as Record<string, unknown>)["gen"], "gen-2");
        flow.handle.stop();
    });
});
