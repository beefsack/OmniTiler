// Integrated wire coverage (strict v2, sole broker): the ACTUAL production
// entry's wire shapes must equal the exact Rust broker contract, and ACTUAL
// Rust-shaped bound replies must parse through the TS parser and apply.
// Offline only: scripted in-memory transport, no D-Bus, no KWin.
//
// Flow under test: DiscoverWorld {v:2,adapter,req} -> bootstrap (broker
// incarnation + gen) -> DescribeWorld envelope
// {v:2,adapter,gen,source,nativeInvalid,report} with a commandless inner
// report -> bound core reply + source/nativeInvalid/clock -> user commands
// via QueueWorldCommand {v:2,client,clientCmd,command} drained by the next
// fresh capture, terminal results requester-only via TakeCommandResult.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";
import {
    buildDiscoveryRequest,
    buildQueueRequest,
    buildTakeRequest,
    isCoreWorldReply,
    isWorldQueueReply,
    isWorldRejection,
    parseWorldReply,
} from "../src/world-report";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

const DISABLED_PRESENTATION = {
    foreground: "",
    border: { visible: false, reason: "disabled", inner: null, outer: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [],
};

function makeSurface() {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [
        {
            internalId: "win-a", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        },
        {
            internalId: "win-b", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws2],
            minimized: true, fullscreen: false, maximized: false, sticky: false,
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
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, world };
}

function lastCall(calls: Call[], method: string): Call {
    const call = [...calls].reverse().find((c) => c.method === method);
    assert.ok(call !== undefined, `expected ${method}`);
    return call as Call;
}

function acceptCapture(gen: string, envelope: Record<string, unknown>, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: envelope["source"], obs: rev, rev,
        ready: true, consumed: false,
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: [{
            win: "win-a", out: "out", ws: "ws1",
            alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "", clear_op: "",
            floating: false, float_origin: "",
        }],
        transfers: [], effects: [], result: null,
        gestureSettled: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: DISABLED_PRESENTATION,
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
        ...extra,
    });
}

function discoverFlow(flow: ReturnType<typeof startFlow>, gen = "gen-1"): { incarnation: string } {
    const disc = lastCall(flow.calls, "DiscoverWorld");
    const bootReq = (JSON.parse(disc.payload) as Record<string, unknown>)["req"] as string;
    assert.deepEqual(JSON.parse(disc.payload), JSON.parse(buildDiscoveryRequest("test-adapter", bootReq)));
    disc.callback(JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: bootReq,
        obs: "0", rev: "0", ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
        gestureSettled: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: DISABLED_PRESENTATION,
    }));
    return { incarnation: "broker-1" };
}

function isCanonicalU64(text: unknown): boolean {
    if (typeof text !== "string" || text.length === 0 || text.length > 20) {
        return false;
    }
    if (text === "0") {
        return true;
    }
    if (!/^[1-9][0-9]*$/.test(text)) {
        return false;
    }
    return !(text.length === 20 && text > "18446744073709551615");
}

// Broker-owned keys must never ride the inner report: exactly one
// sequencing namespace exists service-side.
const BROKER_KEYS = ["owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"];

describe("world wire fixture (TS construction <-> Rust codec)", () => {
    it("production entry discovery is strict v2 and binds the broker incarnation", () => {
        const flow = startFlow();
        const { incarnation } = discoverFlow(flow);
        assert.equal(incarnation, "broker-1");
        // Discovery binds generation: the auto capture carries it.
        const auto = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.equal(body["adapter"], "test-adapter");
        assert.equal(body["gen"], "gen-1");
        assert.ok(isCanonicalU64(body["nativeInvalid"]));
        flow.handle.stop();
    });

    it("production entry capture is a commandless v2 envelope with a full projected inner report", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        // Exact envelope keys, nothing else.
        assert.deepEqual(
            [...Object.keys(body)].sort(),
            ["adapter", "gen", "nativeInvalid", "report", "source", "sourceSeq", "v"].sort(),
        );
        assert.equal(body["v"], 2);
        // Per-capture sequence mints 1-based alongside the source: the
        // first capture of the lifetime carries "1".
        assert.equal(body["sourceSeq"], "1");
        assert.ok(isCanonicalU64(body["sourceSeq"] as string));
        const inner = body["report"] as Record<string, unknown>;
        for (const key of BROKER_KEYS) {
            assert.ok(!(key in inner), `inner report carries no ${key}`);
        }
        // Full v2 facts ride with real producers, never empty defaults.
        assert.ok(Array.isArray(inner["screens"]) && (inner["screens"] as unknown[]).length === 1);
        assert.ok(Array.isArray(inner["windows"]) && (inner["windows"] as unknown[]).length >= 1);
        assert.ok(Array.isArray(inner["views"]));
        assert.ok(Array.isArray(inner["closed"]));
        assert.ok(Array.isArray(inner["native"]));
        assert.equal(inner["hostOwned"], true);
        assert.equal(inner["predicate"], "both-axes-fixed");
        assert.equal(inner["refill"], "last-remaining-workspace");
        assert.equal(inner["defaultTiled"], true);
        const screens = inner["screens"] as Array<Record<string, unknown>>;
        assert.deepEqual(screens[0]?.["full"], [0, 0, 1920, 1080]);
        assert.equal(screens[0]?.["primary"], true);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        // Truthful native workspace evidence: single-homed windows name
        // their actual native desktop, never the logical observed slot.
        const winA = wins.find((w) => w["win"] === "win-a");
        assert.ok(winA !== undefined);
        assert.equal(winA["nativeWs"], "ws1");
        assert.equal(winA["observedWs"], null);
        const minimized = wins.filter((w) => w["minimized"] === true);
        assert.ok(minimized.some((w) => w["win"] === "win-b"));
        flow.handle.stop();
    });

    it("capture sourceSeq mints 1-based and advances per capture in one lifetime", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const first = lastCall(flow.calls, "DescribeWorld");
        const firstBody = JSON.parse(first.payload) as Record<string, unknown>;
        assert.equal(firstBody["sourceSeq"], "1");
        first.callback(acceptCapture("gen-1", firstBody, "1"));
        // A fresh native invalidation dispatches the next capture with the
        // next per-lifetime sequence (same source namespace, never reset
        // on rediscovery under the same producer lifetime).
        flow.handle.requestWorkspaceMove(0, false);
        for (let i = 0; i < flow.timers.length; i += 1) {
            const t = flow.timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                flow.timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
        const queue = lastCall(flow.calls, "QueueWorldCommand");
        const filed = JSON.parse(queue.payload) as Record<string, unknown>;
        queue.callback(JSON.stringify({
            v: 2, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        assert.equal(drainedBody["sourceSeq"], "2");
        assert.notEqual(drainedBody["source"], firstBody["source"]);
        flow.handle.stop();
    });

    it("inner report builder matches the projected broker contract", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const inner = (JSON.parse(auto.payload) as Record<string, unknown>)["report"] as Record<string, unknown>;
        // Re-encode through the shared builder path shape: the same keys,
        // the same v2 version, no broker namespace.
        assert.equal(inner["v"], 2);
        assert.equal(inner["mode"], "per-output-local");
        assert.equal(inner["switchTarget"], "focused-output");
        assert.equal(inner["freshPlacement"], "focused-output-last-focused");
        assert.ok(!("feedback" in inner), "verdict transport deleted, never converted");
        assert.ok(!("frameAttempts" in inner) || Array.isArray(inner["frameAttempts"]), "no fake raw attempts");
        flow.handle.stop();
    });

    it("Rust bound reply bytes parse through the TS parser and apply", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const envelope = JSON.parse(auto.payload) as Record<string, unknown>;
        const replyText = acceptCapture("gen-1", envelope, "1");
        const parsed = parseWorldReply(replyText);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            flow.handle.stop();
            return;
        }
        assert.equal(parsed.source, envelope["source"]);
        assert.equal(parsed.nativeInvalid, envelope["nativeInvalid"]);
        assert.equal(parsed.result, null, "actuator lane never carries the terminal result");
        auto.callback(replyText);
        assert.ok(flow.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")));
        flow.handle.stop();
    });

    it("numbered moves file exact send-numbered commands and take terminal results", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const runTimers = (): void => {
            for (let i = 0; i < flow.timers.length; i += 1) {
                const t = flow.timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120) {
                    flow.timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        // Settle the auto capture so the command flight is idle.
        {
            const auto = lastCall(flow.calls, "DescribeWorld");
            const body = JSON.parse(auto.payload) as Record<string, unknown>;
            auto.callback(acceptCapture("gen-1", body, "0"));
            // Observation-only auto capture files nothing: no take.
            assert.ok(!flow.calls.some((c) => c.method === "TakeCommandResult"));
        }
        flow.handle.requestWorkspaceMove(0, false);
        runTimers();
        const queue = lastCall(flow.calls, "QueueWorldCommand");
        const filed = JSON.parse(queue.payload) as Record<string, unknown>;
        assert.deepEqual([...Object.keys(filed)].sort(), ["client", "clientCmd", "command", "gen", "v"].sort());
        assert.equal(filed["v"], 2);
        assert.equal(filed["gen"], "gen-1", "queue binds the serving generation");
        assert.ok(isCanonicalU64(filed["clientCmd"]));
        const command = filed["command"] as Record<string, unknown>;
        assert.equal(command["op"], "send-numbered");
        assert.equal(command["index"], 0);
        assert.equal(command["follow"], false);
        assert.deepEqual(command["source"], { explicit: "out" });
        // Reply queued: the entry drains with a fresh commandless capture.
        queue.callback(JSON.stringify({
            v: 2, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        assert.ok((drainedBody["source"] as string) !== (JSON.parse(queue.payload) as Record<string, unknown>)["clientCmd"]);
        const drainedReport = drainedBody["report"] as Record<string, unknown>;
        assert.ok(!("command" in drainedReport), "drained capture stays commandless on the wire");
        drained.callback(acceptCapture("gen-1", drainedBody, "2"));
        // Terminal result arrives requester-only via TakeCommandResult.
        const take = lastCall(flow.calls, "TakeCommandResult");
        const takeBody = JSON.parse(take.payload) as Record<string, unknown>;
        assert.deepEqual(takeBody, {
            v: 2, gen: "gen-1", client: filed["client"], clientCmd: filed["clientCmd"],
        });
        take.callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }));
        const parsedTake = parseWorldReply(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }));
        assert.ok(parsedTake !== null && !isWorldRejection(parsedTake) && !isCoreWorldReply(parsedTake) && !isWorldQueueReply(parsedTake));
        flow.handle.stop();
    });

    // ReadWorldPresentation projection: the exact cached-projection key
    // set ({v,outcome,gen,req,rev,obs,scopes,workspaces,outputs,windows,
    // presentation}) parses directly, never through a fabricated core
    // envelope. Write tokens, one-shot lanes, and transport bindings
    // refuse here.
    function projectPresentation(gen: string, req: string, rev: string, obs: string): string {
        return JSON.stringify({
            v: 2, outcome: "presentation", gen, req, rev, obs,
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [{ id: "ws1", out: "out", bind: "ws1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" }],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                floating: false, float_origin: "",
            }],
            presentation: DISABLED_PRESENTATION,
        });
    }

    it("presentation replies parse the real projection key set and refuse one-shot lanes", () => {
        const parsed = parseWorldReply(projectPresentation("gen-1", "req-9", "4", "7"));
        assert.ok(parsed !== null && !isWorldRejection(parsed) && !isCoreWorldReply(parsed) && !isWorldQueueReply(parsed));
        if (parsed === null || isWorldRejection(parsed) || isCoreWorldReply(parsed) || isWorldQueueReply(parsed)) {
            return;
        }
        assert.equal((parsed as { outcome?: unknown }).outcome, "presentation");
        assert.equal((parsed as { gen?: unknown }).gen, "gen-1");
        assert.equal((parsed as { req?: unknown }).req, "req-9");
        assert.deepEqual(
            [...Object.keys(JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")))].sort(),
            ["gen", "obs", "outcome", "outputs", "presentation", "req", "rev", "scopes", "v", "windows", "workspaces"].sort(),
        );
        // Write tokens refuse even when empty: the projection strips them.
        const withToken = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        ((withToken["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["frame_op"] = "";
        assert.equal(parseWorldReply(JSON.stringify(withToken)), null, "frame_op refuses on the presentation lane");
        // One-shot lanes refuse: result, transfers, effects, settlement.
        for (const lane of ["result", "transfers", "effects", "gestureSettled", "operationSettled", "warpCursor"] as const) {
            const withLane = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
            withLane[lane] = lane === "result" || lane === "gestureSettled" || lane === "warpCursor" ? null : [];
            assert.equal(parseWorldReply(JSON.stringify(withLane)), null, `${lane} refuses on the presentation lane`);
        }
        // Transport bindings and readiness flags belong to other lanes.
        for (const lane of ["adapter", "source", "nativeInvalid", "clock", "ready", "consumed"] as const) {
            const withLane = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
            withLane[lane] = lane === "ready" || lane === "consumed" ? false : "x";
            assert.equal(parseWorldReply(JSON.stringify(withLane)), null, `${lane} refuses on the presentation lane`);
        }
        // Core full-state shape never parses as presentation (no outcome).
        const coreShaped = parseWorldReply(acceptCapture("gen-1", {
            v: 2, adapter: "test-adapter", gen: "gen-1",
            source: "src-1", sourceSeq: "1", nativeInvalid: "1",
            report: { v: 2 },
        }, "1"));
        assert.ok(coreShaped !== null && isCoreWorldReply(coreShaped));
    });

    it("a timed-out queue redelivers the identical queue bytes under the same clientCmd", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const runTimers = (): void => {
            for (let i = 0; i < flow.timers.length; i += 1) {
                const t = flow.timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120) {
                    flow.timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        {
            const auto = lastCall(flow.calls, "DescribeWorld");
            const body = JSON.parse(auto.payload) as Record<string, unknown>;
            auto.callback(acceptCapture("gen-1", body, "0"));
        }
        flow.handle.requestWorkspaceMove(0, false);
        runTimers();
        const firstQueue = lastCall(flow.calls, "QueueWorldCommand");
        const firstPayload = firstQueue.payload;
        const firstBody = JSON.parse(firstPayload) as Record<string, unknown>;
        // Expire the queue flight without answering: the transport reply
        // is lost, so the filed-but-unacked command stays unresolved and
        // nothing files a duplicate. (The harness cancel is a no-op, so
        // stale discovery/capture timeouts linger: the queue timeout is
        // the latest 2000ms arm.)
        const timeout = [...flow.timers].reverse().find((t) => t.delay === 2000);
        assert.ok(timeout !== undefined, "queue timeout armed");
        flow.timers.splice(flow.timers.indexOf(timeout as { delay: number; cb: () => void }), 1);
        (timeout as { cb: () => void }).cb();
        assert.ok(!flow.calls.some((c) => c.method === "TakeCommandResult"), "lost queue files no take");
        // The next event-driven dispatch redelivers THE SAME queue bytes
        // (the broker high-water dedups the same clientCmd, never
        // re-executes) instead of minting a numbered duplicate.
        flow.handle.requestWorkspaceMove(1, false);
        runTimers();
        const queues = flow.calls.filter((c) => c.method === "QueueWorldCommand");
        assert.equal(queues.length, 2, "exactly one redelivery, no duplicate numbering");
        assert.equal((queues[1] as { payload: string }).payload, firstPayload, "redelivery is byte-identical");
        (queues[1] as { callback: (reply: unknown) => void }).callback(JSON.stringify({
            v: 2, outcome: "queued", gen: "gen-1",
            client: firstBody["client"], clientCmd: firstBody["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        drained.callback(acceptCapture("gen-1", drainedBody, "2"));
        // The terminal take collects exactly once under the original
        // identity (the broker re-serves the redelivered filing).
        const takes = flow.calls.filter((c) => c.method === "TakeCommandResult");
        assert.equal(takes.length, 1, "one terminal take for the redelivered command");
        const takeBody = JSON.parse((takes[0] as { payload: string }).payload) as Record<string, unknown>;
        assert.equal(takeBody["clientCmd"], firstBody["clientCmd"]);
        (takes[0] as { callback: (reply: unknown) => void }).callback(
            JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }),
        );
        flow.handle.stop();
    });

    it("a pending take holds the snapshot while a null ready settles success", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const runTimers = (): void => {
            for (let i = 0; i < flow.timers.length; i += 1) {
                const t = flow.timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120) {
                    flow.timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        {
            const auto = lastCall(flow.calls, "DescribeWorld");
            const body = JSON.parse(auto.payload) as Record<string, unknown>;
            auto.callback(acceptCapture("gen-1", body, "0"));
        }
        // Codec level: pending and ready-null are distinct terminal
        // shapes; a null result is success, never a failure.
        assert.deepEqual(
            parseWorldReply('{"v":2,"outcome":"pending","gen":"gen-1"}'),
            { outcome: "pending", gen: "gen-1" },
        );
        const readyNull = parseWorldReply('{"v":2,"outcome":"ready","gen":"gen-1","globalCmd":"3","result":null}');
        assert.ok(readyNull !== null && (readyNull as { outcome?: unknown }).outcome === "ready");
        // Entry level: a pending take still applies the accepted snapshot
        // (the command stays undecided server-side) and a later null
        // ready settles cleanly with no terminal handler doubling.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers();
        const queue = lastCall(flow.calls, "QueueWorldCommand");
        const filed = JSON.parse(queue.payload) as Record<string, unknown>;
        queue.callback(JSON.stringify({
            v: 2, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        drained.callback(acceptCapture("gen-1", drainedBody, "2"));
        const take = lastCall(flow.calls, "TakeCommandResult");
        take.callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
        assert.ok(flow.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")), "pending take still applies");
        flow.handle.stop();
    });

    it("core replies refuse the retired v1 workspace field and shape drift", () => {
        // The retired v1 `logical` workspace key refuses even when every
        // other byte matches the checked-in golden reply: exact v2 shape,
        // no v1 extras.
        const golden = JSON.parse(readFileSync(join("..", "test-fixtures", "world-wire", "reply-golden.json"), "utf8")) as Record<string, unknown>;
        const workspaces = golden["workspaces"] as Array<Record<string, unknown>>;
        assert.ok(workspaces.length > 0);
        const stale = {
            ...golden,
            workspaces: workspaces.map((w) => ({ ...w, logical: "desk-1" })),
        };
        assert.equal(parseWorldReply(JSON.stringify(stale)), null, "retired logical refuses");
        assert.equal(
            parseWorldReply(JSON.stringify({ ...golden, extra: 1 })),
            null,
            "unknown top-level field refuses",
        );
        const missing = { ...golden } as Record<string, unknown>;
        delete missing["clock"];
        assert.equal(parseWorldReply(JSON.stringify(missing)), null, "missing binding refuses");
        const parsed = parseWorldReply(JSON.stringify(golden));
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "canonical broker bytes parse");
    });

    it("queue/take codecs refuse v1 and unknown fields", () => {        assert.equal(
            parseWorldReply('{"v":1,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"1"}'),
            null,
        );
        assert.equal(
            parseWorldReply('{"v":2,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"1","extra":1}'),
            null,
        );
        assert.equal(
            parseWorldReply('{"v":2,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"03","globalCmd":"1"}'),
            null,
            "non-canonical clientCmd refuses",
        );
        const takeReq = JSON.parse(buildTakeRequest("c", "gen-1", "4")) as Record<string, unknown>;
        assert.deepEqual(takeReq, { v: 2, gen: "gen-1", client: "c", clientCmd: "4" });
        assert.equal(
            parseWorldReply('{"v":2,"outcome":"ready","gen":"gen-1","globalCmd":"07","result":null}'),
            null,
        );
        assert.ok(isWorldQueueReply(parseWorldReply(
            '{"v":2,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"9"}',
        )));
        const queueReq = JSON.parse(buildQueueRequest("c", "gen-1", "1", { op: "focus", win: "w", direction: "left" })) as Record<string, unknown>;
        assert.deepEqual(queueReq, { v: 2, gen: "gen-1", client: "c", clientCmd: "1", command: { op: "focus", win: "w", direction: "left" } });
    });
});
