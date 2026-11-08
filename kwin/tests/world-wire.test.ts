// Integrated wire coverage (strict v4, sole broker): the ACTUAL production
// entry's handshake/settings plus wire shapes must equal the exact v4 broker
// contract, and ACTUAL Rust-shaped bound replies must parse through the TS
// parser and apply. Offline only: scripted in-memory transport, no D-Bus, no KWin.
//
// Flow under test: handshake plus revisioned settings on DiscoverWorld (typed
// connection family, settings accepted before the first report) ->
// DescribeWorld envelope {v:4,adapter,gen,source,sourceSeq,nativeInvalid} with
// a commandless v4 native-fact inner report -> bound core reply +
// source/nativeInvalid/clock -> user commands via QueueWorldCommand
// {v:4,client,clientCmd,command} drained by the next fresh capture, terminal
// results requester-only via TakeCommandResult.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";
import {
    buildQueueRequest,
    buildTakeRequest,
    isCoreWorldReply,
    isWorldQueueReply,
    isWorldRejection,
    parseWorldReply,
} from "../src/world-report";
import { buildHandshakeRequest, defaultKwinCapabilities, ownApplicationIdentity } from "../src/world-connection";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

const DISABLED_PRESENTATION = {
    foreground: "",
    rev: "0",
    border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [],
};

function makeSurface() {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window-a", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-b", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws2],
            minimized: true, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window-b", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
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
    // Synthetic strict-v4 receipt; obs/rev are independent of nativeInvalid.
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: envelope["source"], obs: rev, rev,
        ready: true, consumed: false,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: [{
            win: "win-a", out: "out", ws: "ws1",
            alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "", clear_op: "",
            floating: false, float_origin: "",
            cmdSeq: null,
            visibility: null, visibility_auth: "untouched", visibility_hold: "",
            state: [],
        }],
        transfers: [], effects: [], result: null,
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored capture accept binds strict v4");
    return text;
}

function discoverFlow(flow: ReturnType<typeof startFlow>, gen = "gen-1"): { incarnation: string } {
    // Synthetic connection receipts accept settings before the first report.
    const hs = flow.calls.find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch (error) {
            void error;
            return false;
        }
    });
    assert.ok(hs !== undefined, "expected DiscoverWorld/handshake");
    const hsBody = JSON.parse((hs as Call).payload) as Record<string, unknown>;
    assert.equal(hsBody["type"], "handshake");
    assert.equal(hsBody["v"], 4);
    assert.equal(hsBody["adapter"], "test-adapter");
    const hsReq = hsBody["req"] as string;
    assert.deepEqual(hsBody["applicationIdentity"], { appId: null, executable: null, processId: "kwin-internal" });
    const caps = hsBody["capabilities"] as Record<string, unknown>;
    assert.equal(caps["hostOwned"], true);
    assert.equal(caps["equalCount"], true);
    assert.deepEqual(caps["unavailableFacts"], [...defaultKwinCapabilities(true).unavailableFacts]);
    assert.equal(
        (hs as Call).payload,
        buildHandshakeRequest("test-adapter", hsReq, ownApplicationIdentity(), defaultKwinCapabilities(true)),
        "handshake bytes equal the exact builder",
    );
    (hs as Call).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req: hsReq,
        ready: false, kind: "", gen,
    }));
    // No report flies before the accepted settings: the first capture waits
    // for the authoritative snapshot.
    assert.ok(!flow.calls.some((c) => c.method === "DescribeWorld"), "no report before accepted settings");
    const set = [...flow.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch (error) {
            void error;
            return false;
        }
    });
    assert.ok(set !== undefined, "expected DiscoverWorld/settings");
    const setBody = JSON.parse((set as Call).payload) as Record<string, unknown>;
    assert.equal(setBody["type"], "settings");
    assert.equal(setBody["v"], 4);
    assert.equal(setBody["adapter"], "test-adapter");
    assert.equal(setBody["gen"], gen);
    assert.equal(setBody["rev"], "1");
    assert.ok(isCanonicalU64(setBody["rev"]));
    assert.equal((setBody["values"] as Record<string, unknown>)["mode"], "per-output-local");
    (set as Call).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: setBody["req"], rev: setBody["rev"], status: "accepted", diagnostics: [],
    }));
    for (let i = 0; i < flow.timers.length; i += 1) {
        const t = flow.timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            flow.timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
    return { incarnation: "test-adapter" };
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
    it("production entry discovery is strict v4 handshake plus settings binding the producer adapter", () => {
        const flow = startFlow();
        const { incarnation } = discoverFlow(flow);
        assert.equal(incarnation, "test-adapter");
        // Discovery binds the producer adapter lifetime (never a broker
        // incarnation): the auto capture carries it and the generation.
        const auto = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        assert.equal(body["v"], 4);
        assert.equal(body["adapter"], "test-adapter");
        assert.equal(body["gen"], "gen-1");
        assert.ok(isCanonicalU64(body["nativeInvalid"]));
        flow.handle.stop();
    });

    it("production entry capture is a commandless v4 envelope with genuine native-fact inner report", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        // Exact envelope keys, nothing else.
        assert.deepEqual(
            [...Object.keys(body)].sort(),
            ["adapter", "gen", "nativeInvalid", "report", "source", "sourceSeq", "v"].sort(),
        );
        assert.equal(body["v"], 4);
        // Per-capture sequence mints 1-based alongside the source: the
        // first capture of the lifetime carries "1".
        assert.equal(body["sourceSeq"], "1");
        assert.ok(isCanonicalU64(body["sourceSeq"] as string));
        const inner = body["report"] as Record<string, unknown>;
        for (const key of BROKER_KEYS) {
            assert.ok(!(key in inner), `inner report carries no ${key}`);
        }
        // Genuine v4 native facts ride with real producers, never empty
        // defaults and never broker-classified verdicts.
        assert.ok(Array.isArray(inner["screens"]) && (inner["screens"] as unknown[]).length === 1);
        assert.ok(Array.isArray(inner["windows"]) && (inner["windows"] as unknown[]).length >= 1);
        assert.ok(Array.isArray(inner["closed"]));
        assert.ok(Array.isArray(inner["native"]));
        assert.ok("focused" in inner, "v4 focus lane rides");
        for (const retired of ["mode", "views", "hostOwned", "equalCount", "birthClear", "predicate", "refill",
            "bornMaximized", "defaultTiled", "switchTarget", "freshPlacement", "focusFollowsCursor", "cursorWarp",
            "anchorFallback", "noSpaceResult", "floatIntent", "tileIntent", "primary", "gestureIngress"]) {
            assert.ok(!(retired in inner), `retired v3 ${retired} never rides the v4 report`);
        }
        const screens = inner["screens"] as Array<Record<string, unknown>>;
        assert.deepEqual(screens[0]?.["full"], [0, 0, 1920, 1080]);
        assert.deepEqual(screens[0]?.["work"], [0, 0, 800, 600]);
        assert.ok(!("primary" in (screens[0] as Record<string, unknown>)), "active screen is not primary proof");
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        // Truthful native workspace evidence: single-homed windows carry
        // their actual native desktop id, never a logical slot.
        const winA = wins.find((w) => w["win"] === "win-a");
        assert.ok(winA !== undefined);
        assert.equal(winA["nativeWs"], "ws1");
        assert.deepEqual(winA["rect"], [0, 0, 200, 200]);
        assert.deepEqual(winA["hints"], { minW: null, minH: null, maxW: null, maxH: null });
        assert.deepEqual(winA["visibility"], { visible: null, suppressionOwned: false });
        for (const row of wins) {
            for (const legacy of ["floating", "fitExcluded", "fixedAuto", "fixedSuppress", "frameGeometry",
                "clientGeometry", "desktops", "minSize", "maxSize", "fullScreen", "maximizeMode", "observedWs"]) {
                assert.ok(!(legacy in row), `v4 row carries no legacy ${legacy}`);
            }
        }
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
            v: 4, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        assert.equal(drainedBody["sourceSeq"], "2");
        assert.notEqual(drainedBody["source"], firstBody["source"]);
        assert.equal(drainedBody["nativeInvalid"], firstBody["nativeInvalid"], "capture sequence advances without native invalidation");
        flow.handle.stop();
    });

    it("inner report builder carries the v4 native contract without broker namespace or legacy verdicts", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const inner = (JSON.parse(auto.payload) as Record<string, unknown>)["report"] as Record<string, unknown>;
        // The exact v4 producer shape: version 4, genuine native facts, no
        // broker namespace, no retired v3 keys, no classifier verdicts.
        assert.equal(inner["v"], 4);
        assert.ok(Array.isArray(inner["screens"]) && Array.isArray(inner["windows"]) && Array.isArray(inner["native"]));
        for (const key of BROKER_KEYS) {
            assert.ok(!(key in inner), `inner report carries no ${key}`);
        }
        for (const retired of ["mode", "views", "hostOwned", "equalCount", "predicate", "refill", "defaultTiled",
            "switchTarget", "freshPlacement", "anchorFallback", "noSpaceResult"]) {
            assert.ok(!(retired in inner), `retired v3 ${retired} never rides the v4 report`);
        }
        assert.ok(!("feedback" in inner), "verdict transport deleted, never converted");
        // Unpopulated attempt lanes omit (never null): omission backfills
        // the observation-only default.
        for (const lane of ["frameAttempts", "memberAttempts", "viewAttempts", "clearAttempts", "creationAttempts"]) {
            assert.ok(!(lane in inner), `unmeasured ${lane} omits, never nulls`);
        }
        flow.handle.stop();
    });

    it("synthetic bound v4 reply parses, applies, and stale revs apply nothing", () => {
        const flow = startFlow();
        discoverFlow(flow);
        const auto = lastCall(flow.calls, "DescribeWorld");
        const envelope = JSON.parse(auto.payload) as Record<string, unknown>;
        // The bound obs runs independently of the native watermark.
        const replyText = acceptCapture("gen-1", envelope, "1", { obs: "9" });
        const parsed = parseWorldReply(replyText);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            flow.handle.stop();
            return;
        }
        assert.equal(parsed.obs, "9", "obs independent of the native watermark");
        assert.equal(parsed.source, envelope["source"]);
        assert.equal(parsed.nativeInvalid, envelope["nativeInvalid"]);
        assert.equal(parsed.result, null, "actuator lane never carries the terminal result");
        auto.callback(replyText);
        assert.ok(flow.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")));
        // A stale rev on a newer watermark applies nothing further.
        const writes = flow.world.geometryWrites.length;
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
            v: 4, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        drained.callback(acceptCapture("gen-1", drainedBody, "1"));
        assert.equal(flow.world.geometryWrites.length, writes, "stale rev applies nothing");
        // The parser decodes correlated results truthfully (never a parser
        // default): success omits the no-space marker on the wire and fills
        // false, so actuator-side null-stripping is broker behavior only.
        // Exact vectors are immutable: every mutation works on a clone.
        const dir = join("..", "test-fixtures", "world-wire", "v4", "messages");
        const floatBody = JSON.parse(readFileSync(join(dir, "reply-result-float-placed.json"), "utf8")) as Record<string, unknown>;
        assert.ok(!("noSpace" in (floatBody["result"] as Record<string, unknown>)), "success omits noSpace on the wire");
        const boundFloat = JSON.stringify({ ...floatBody, source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null });
        const parsedFloat = parseWorldReply(boundFloat);
        assert.ok(parsedFloat !== null && isCoreWorldReply(parsedFloat));
        if (!isCoreWorldReply(parsedFloat)) {
            flow.handle.stop();
            return;
        }
        assert.deepEqual(parsedFloat.result, { op: "toggle-float", win: "win-a", rect: [10, 20, 640, 480], noSpace: false });
        const strippedFloat = JSON.stringify({ ...floatBody, result: null, source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null });
        const parsedStripped = parseWorldReply(strippedFloat);
        assert.ok(parsedStripped !== null && isCoreWorldReply(parsedStripped));
        if (!isCoreWorldReply(parsedStripped)) {
            flow.handle.stop();
            return;
        }
        assert.equal(parsedStripped.result, null, "broker-stripped result binds null");
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
        assert.equal(filed["v"], 4);
        assert.equal(filed["gen"], "gen-1", "queue binds the serving generation");
        assert.ok(isCanonicalU64(filed["clientCmd"]));
        const command = filed["command"] as Record<string, unknown>;
        assert.equal(command["op"], "send-numbered");
        // Ordinary keyboard intents resolve implicit at core current focus
        // (docs/decisions/commands.md#kde-rapid-input).
        assert.deepEqual(command["win"], { currentFocus: true });
        assert.equal(command["index"], 0);
        assert.equal(command["follow"], false);
        assert.deepEqual(command["source"], { implicit: "home-then-action" });
        // Reply queued: the entry drains with a fresh commandless capture.
        queue.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        assert.ok((drainedBody["source"] as string) !== (JSON.parse(queue.payload) as Record<string, unknown>)["clientCmd"]);
        assert.ok(!("command" in drainedBody), "drained envelope stays commandless on the wire");
        const drainedReport = drainedBody["report"] as Record<string, unknown>;
        assert.ok(!("command" in drainedReport), "drained capture stays commandless on the wire");
        drained.callback(acceptCapture("gen-1", drainedBody, "2"));
        // Terminal result arrives requester-only via TakeCommandResult.
        const take = lastCall(flow.calls, "TakeCommandResult");
        const takeBody = JSON.parse(take.payload) as Record<string, unknown>;
        assert.deepEqual(takeBody, {
            v: 4, gen: "gen-1", client: filed["client"], clientCmd: filed["clientCmd"],
        });
        take.callback(JSON.stringify({ v: 4, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }));
        const parsedTake = parseWorldReply(JSON.stringify({ v: 4, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }));
        assert.ok(parsedTake !== null && !isWorldRejection(parsedTake) && !isCoreWorldReply(parsedTake) && !isWorldQueueReply(parsedTake));
        flow.handle.stop();
    });

    // ReadWorldPresentation projection: the exact cached-projection key
    // set ({v,outcome,gen,req,rev,obs,coveredInstance,coveredInvalid,
    // scopes,workspaces,outputs,windows,presentation,preview}) parses
    // directly, never through a fabricated core envelope. Write tokens,
    // one-shot lanes, and transport bindings refuse here. `preview` is the
    // trimmed renderer-only drop preview (paint rect, mover-home source,
    // insertion-neighbor win, producing-gesture identity or nulls when
    // unproven) or null. `coveredInstance`/`coveredInvalid` name the causal
    // coverage the cached capture covered (nulls when unproven).
    function projectPresentation(gen: string, req: string, rev: string, obs: string): string {
        return JSON.stringify({
            v: 4, outcome: "presentation", gen, req, rev, obs,
            coveredInstance: "effect-1", coveredInvalid: "5",
            scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
            workspaces: [{ id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" }],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                floating: false, float_origin: "",
                cmdSeq: null, visibility: null,
                visibility_auth: "untouched", visibility_hold: "",
                state: [],
            }],
            presentation: DISABLED_PRESENTATION,
            preview: null,
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
            ["gen", "obs", "outcome", "outputs", "presentation", "preview", "req", "rev", "coveredInstance", "coveredInvalid", "scopes", "v", "windows", "workspaces"].sort(),
        );
        assert.deepEqual((parsed as { preview?: unknown }).preview, null);
        const covered = parsed as { coveredInstance?: unknown; coveredInvalid?: unknown };
        assert.equal(covered.coveredInstance, "effect-1");
        assert.equal(covered.coveredInvalid, "5");
        // The trimmed renderer-only drop preview parses (paint rect plus
        // subject proof plus the producing identity); issuer/provenance
        // extras refuse. `win` is the insertion neighbor, never the mover
        // identity.
        const withPreview = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        withPreview["preview"] = {
            rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a",
            producerInstance: "gesture-1", producerSerial: "1",
        };
        const parsedPreview = parseWorldReply(JSON.stringify(withPreview));
        assert.ok(parsedPreview !== null && !isWorldRejection(parsedPreview) && !isCoreWorldReply(parsedPreview) && !isWorldQueueReply(parsedPreview));
        assert.deepEqual((parsedPreview as { preview?: unknown }).preview, withPreview["preview"]);
        // An unproven preview (null producer) parses too: the geometry
        // rides the projection while the renderer hides it.
        const unproven = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        unproven["preview"] = {
            rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a",
            producerInstance: null, producerSerial: null,
        };
        assert.ok(parseWorldReply(JSON.stringify(unproven)) !== null, "null producer parses");
        for (const bad of [
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1" },
            { rect: [0, 0, 396], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "gesture-1", producerSerial: "1" },
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "gesture-1", producerSerial: "1", op: "drag-preview" },
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "gesture-1", producerSerial: null },
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "gesture-1", producerSerial: "0" },
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "", producerSerial: "1" },
            { rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws-1", win: "win-a", producerInstance: "gesture-1", producerSerial: "1", hover: null },
        ]) {
            const badReply = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
            badReply["preview"] = bad;
            assert.equal(parseWorldReply(JSON.stringify(badReply)), null, "malformed preview refuses");
        }
        // Write tokens refuse even when empty: the projection strips them.
        const withToken = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        ((withToken["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["frame_op"] = "";
        assert.equal(parseWorldReply(JSON.stringify(withToken)), null, "frame_op refuses on the presentation lane");
        // One-shot lanes refuse: result, transfers, effects, settlement.
        for (const lane of ["result", "transfers", "effects", "gestureSettled", "operationSettled", "warpCursor", "intentWrites", "switchSettled", "diagnostics"] as const) {
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
            v: 4, adapter: "test-adapter", gen: "gen-1",
            source: "src-1", sourceSeq: "1", nativeInvalid: "1",
            report: { v: 4 },
        }, "1"));
        assert.ok(coreShaped !== null && isCoreWorldReply(coreShaped));
    });

    it("presentation parses the optional focused-fullscreen suspension verdict", () => {
        // Absent key (default publication) parses to a null verdict.
        const plain = parseWorldReply(projectPresentation("gen-1", "req-9", "4", "7"));
        assert.ok(plain !== null && !isWorldRejection(plain) && !isCoreWorldReply(plain) && !isWorldQueueReply(plain));
        assert.equal((plain as { presentation?: { suspendedOutput?: unknown } }).presentation?.suspendedOutput, null);
        // Present verdict parses verbatim.
        const withVerdict = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        (withVerdict["presentation"] as Record<string, unknown>)["suspendedOutput"] = "out-1";
        const parsed = parseWorldReply(JSON.stringify(withVerdict));
        assert.ok(parsed !== null && !isWorldRejection(parsed) && !isCoreWorldReply(parsed) && !isWorldQueueReply(parsed));
        assert.equal((parsed as { presentation?: { suspendedOutput?: unknown } }).presentation?.suspendedOutput, "out-1");
        // Explicit null refuses: absent fills null, present null is malformed.
        const withNull = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        (withNull["presentation"] as Record<string, unknown>)["suspendedOutput"] = null;
        assert.equal(parseWorldReply(JSON.stringify(withNull)), null, "explicit null refuses");
        // Malformed verdicts and unknown fifth keys refuse.
        for (const bad of ["", "bad id!", 7, true, [], {}]) {
            const badReply = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
            (badReply["presentation"] as Record<string, unknown>)["suspendedOutput"] = bad;
            assert.equal(parseWorldReply(JSON.stringify(badReply)), null, `malformed verdict refuses: ${JSON.stringify(bad)}`);
        }
        const extra = JSON.parse(projectPresentation("gen-1", "req-9", "4", "7")) as Record<string, unknown>;
        (extra["presentation"] as Record<string, unknown>)["suspendedOutput"] = "out-1";
        (extra["presentation"] as Record<string, unknown>)["unknown"] = 1;
        assert.equal(parseWorldReply(JSON.stringify(extra)), null, "unknown fifth key refuses");
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
            v: 4, outcome: "queued", gen: "gen-1",
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
            JSON.stringify({ v: 4, outcome: "ready", gen: "gen-1", globalCmd: "1", result: null }),
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
            parseWorldReply('{"v":4,"outcome":"pending","gen":"gen-1"}'),
            { outcome: "pending", gen: "gen-1" },
        );
        const readyNull = parseWorldReply('{"v":4,"outcome":"ready","gen":"gen-1","globalCmd":"3","result":null}');
        assert.ok(readyNull !== null && (readyNull as { outcome?: unknown }).outcome === "ready");
        // Entry level: a pending take still applies the accepted snapshot
        // (the command stays undecided server-side) and a later null
        // ready settles cleanly with no terminal handler doubling.
        flow.handle.requestWorkspaceMove(0, false);
        runTimers();
        const queue = lastCall(flow.calls, "QueueWorldCommand");
        const filed = JSON.parse(queue.payload) as Record<string, unknown>;
        queue.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: filed["client"], clientCmd: filed["clientCmd"], globalCmd: "1",
        }));
        const drained = lastCall(flow.calls, "DescribeWorld");
        const drainedBody = JSON.parse(drained.payload) as Record<string, unknown>;
        drained.callback(acceptCapture("gen-1", drainedBody, "2"));
        const take = lastCall(flow.calls, "TakeCommandResult");
        take.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
        assert.ok(flow.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")), "pending take still applies");
        flow.handle.stop();
    });

    it("core replies refuse the retired v3 workspace field and shape drift", () => {
        // Normative v4 vector plus bound transport: retired keys and dropped
        // required lanes refuse, the canonical bytes parse.
        const dir = join("..", "test-fixtures", "world-wire", "v4", "messages");
        const vector = JSON.parse(readFileSync(join(dir, "reply-observe.json"), "utf8")) as Record<string, unknown>;
        const bound = (): Record<string, unknown> => ({
            ...JSON.parse(JSON.stringify(vector)) as Record<string, unknown>,
            source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null,
        });
        const stale = bound();
        for (const row of stale["workspaces"] as Array<Record<string, unknown>>) {
            row["logical"] = "desk-1";
        }
        assert.equal(parseWorldReply(JSON.stringify(stale)), null, "retired logical refuses");
        assert.equal(
            parseWorldReply(JSON.stringify({ ...bound(), extra: 1 })),
            null,
            "unknown top-level field refuses",
        );
        const missing = bound();
        delete missing["intentWrites"];
        assert.equal(parseWorldReply(JSON.stringify(missing)), null, "missing intentWrites refuses");
        const missingClock = bound();
        delete missingClock["clock"];
        assert.equal(parseWorldReply(JSON.stringify(missingClock)), null, "missing binding refuses");
        const missingAssoc = bound();
        delete missingAssoc["gestureAssoc"];
        assert.equal(parseWorldReply(JSON.stringify(missingAssoc)), null, "missing gestureAssoc refuses");
        const missingSeq = bound();
        for (const row of missingSeq["windows"] as Array<Record<string, unknown>>) {
            delete row["cmdSeq"];
        }
        assert.equal(parseWorldReply(JSON.stringify(missingSeq)), null, "missing cmdSeq refuses");
        const parsed = parseWorldReply(JSON.stringify(bound()));
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "canonical bound vector parses");
    });

    it("trailing marker parses absent-false, true, and refuses present-false or non-boolean", () => {
        const dir = join("..", "test-fixtures", "world-wire", "v4", "messages");
        const vector = JSON.parse(readFileSync(join(dir, "reply-observe.json"), "utf8")) as Record<string, unknown>;
        const clone = (): Record<string, unknown> => ({
            ...JSON.parse(JSON.stringify(vector)) as Record<string, unknown>,
            source: "src-a", nativeInvalid: "1", clock: "2", gestureAssoc: null,
        });
        const pagerRows = (boundReply: Record<string, unknown>): Array<Record<string, unknown>> => {
            const out: Array<Record<string, unknown>> = [];
            for (const output of (boundReply["presentation"] as Record<string, unknown>)["pager"] as Array<Record<string, unknown>>) {
                for (const row of output["workspaces"] as Array<Record<string, unknown>>) {
                    out.push(row);
                }
            }
            return out;
        };
        // Absent key on either lane fills false.
        const unmarked = clone();
        for (const row of unmarked["workspaces"] as Array<Record<string, unknown>>) {
            delete row["trailing"];
        }
        for (const row of pagerRows(unmarked)) {
            delete row["trailing"];
        }
        const legacy = parseWorldReply(JSON.stringify(unmarked));
        assert.ok(legacy !== null && isCoreWorldReply(legacy));
        if (!isCoreWorldReply(legacy)) {
            return;
        }
        assert.ok(legacy.workspaces.length > 0);
        assert.ok(legacy.workspaces.every((w) => w.trailing === false));
        assert.ok(legacy.presentation.pager.length > 0);
        assert.ok(legacy.presentation.pager.every((s) => s.workspaces.every((w) => w.trailing === false)));
        // Explicit true marks on both lanes.
        const marked = clone();
        ((marked["workspaces"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["trailing"] = true;
        (pagerRows(marked)[0] as Record<string, unknown>)["trailing"] = true;
        const parsedMarked = parseWorldReply(JSON.stringify(marked));
        assert.ok(parsedMarked !== null && isCoreWorldReply(parsedMarked));
        if (!isCoreWorldReply(parsedMarked)) {
            return;
        }
        assert.equal(parsedMarked.workspaces[0]?.trailing, true);
        assert.equal(parsedMarked.presentation.pager[0]?.workspaces[0]?.trailing, true);
        // Present-but-false refuses on either lane: the marker rides only
        // when true.
        const falseWs = clone();
        ((falseWs["workspaces"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["trailing"] = false;
        assert.equal(parseWorldReply(JSON.stringify(falseWs)), null, "workspace trailing false refuses");
        const falsePager = clone();
        (pagerRows(falsePager)[0] as Record<string, unknown>)["trailing"] = false;
        assert.equal(parseWorldReply(JSON.stringify(falsePager)), null, "pager trailing false refuses");
        // Non-boolean markers refuse on either lane.
        for (const bad of ["yes", 1]) {
            const badWs = clone();
            ((badWs["workspaces"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["trailing"] = bad;
            assert.equal(parseWorldReply(JSON.stringify(badWs)), null, `workspace trailing refuses ${JSON.stringify(bad)}`);
            const badPager = clone();
            (pagerRows(badPager)[0] as Record<string, unknown>)["trailing"] = bad;
            assert.equal(parseWorldReply(JSON.stringify(badPager)), null, `pager trailing refuses ${JSON.stringify(bad)}`);
        }
    });

    it("queue/take codecs refuse stale versions and unknown fields", () => {
        for (const v of [1, 2, 3]) {
            assert.equal(
                parseWorldReply(`{"v":${v},"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"1"}`),
                null,
                `v${v} queued refuses`,
            );
        }
        assert.equal(
            parseWorldReply('{"v":4,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"1","extra":1}'),
            null,
        );
        assert.equal(
            parseWorldReply('{"v":4,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"03","globalCmd":"1"}'),
            null,
            "non-canonical clientCmd refuses",
        );
        const takeReq = JSON.parse(buildTakeRequest("c", "gen-1", "4")) as Record<string, unknown>;
        assert.deepEqual(takeReq, { v: 4, gen: "gen-1", client: "c", clientCmd: "4" });
        assert.equal(
            parseWorldReply('{"v":4,"outcome":"ready","gen":"gen-1","globalCmd":"07","result":null}'),
            null,
        );
        assert.ok(isWorldQueueReply(parseWorldReply(
            '{"v":4,"outcome":"queued","gen":"gen-1","client":"c","clientCmd":"1","globalCmd":"9"}',
        )));
        const queueReq = JSON.parse(buildQueueRequest("c", "gen-1", "1", { op: "focus", win: "w" })) as Record<string, unknown>;
        assert.deepEqual(queueReq, { v: 4, gen: "gen-1", client: "c", clientCmd: "1", command: { op: "focus", win: "w" } });
    });
});
