// K1a connection foundation (strict v4 handshake + revisioned settings):
// exact wire shapes, correlation/capabilities, settings-before-reports
// ordering, fresh-generation cleanup, bound wait-deadline no-handshake
// regression, and generation-mismatch reconnect. Offline only: scripted
// in-memory transport, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";
import { isOpaqueId } from "../src/world-seq";
import {
    KWIN_UNAVAILABLE_FACTS,
    OWN_PROCESS_ID_TOKEN,
    buildHandshakeRequest,
    buildSettingsRequest,
    defaultKwinCapabilities,
    mapNativePid,
    ownApplicationIdentity,
    parseConnectionError,
    parseHandshakeReply,
    parseSettingsReply,
} from "../src/world-connection";

const GEN = "gen-1";

function handshakeAccept(adapter: string, req: string, gen: string): string {
    return JSON.stringify({
        type: "handshake-reply", v: 4, adapter, req, gen, ready: false, kind: "",
    });
}

function settingsAccept(adapter: string, gen: string, req: string, rev: string, status = "accepted"): string {
    return JSON.stringify({
        type: "settings-reply", v: 4, adapter, gen, req, rev, status, diagnostics: [],
    });
}

function boundMismatch(gen: string): string {
    return JSON.stringify({
        v: 4, outcome: "rejected", kind: "generation-mismatch",
        message: "generation-mismatch", gen,
        source: "", nativeInvalid: "0", clock: "7", gestureAssoc: null,
    });
}

describe("v4 connection codecs (handshake + settings)", () => {
    it("own identity is null appId/executable with the fixed grammar-valid token", () => {
        const own = ownApplicationIdentity();
        assert.equal(own.appId, null);
        assert.equal(own.executable, null);
        assert.equal(own.processId, OWN_PROCESS_ID_TOKEN);
        assert.ok(isOpaqueId(OWN_PROCESS_ID_TOKEN), "fixed token is Id grammar-valid");
    });

    it("capabilities are truthful host limits with floor-true scope and no unowned hide/above", () => {
        // Floor default (KWin 6.6 has no per-output API): all-outputs.
        const floor = defaultKwinCapabilities();
        assert.equal(floor.nativeSelectorScope, "all-outputs");
        // A 6.7 host with the per-output read method claims per-output.
        const perOutput = defaultKwinCapabilities(true);
        assert.equal(perOutput.nativeSelectorScope, "per-output");
        const caps = floor;
        assert.equal(caps.hostOwned, true);
        assert.equal(caps.equalCount, true);
        assert.equal(caps.birthClear, true);
        assert.equal(caps.warp, false);
        assert.equal(caps.tiledFloatTransfer, true);
        // No independent recoverable per-window hide actuator exists
        // (minimized holds fail closed; hidden only aliases eligibility),
        // so suppression stays false and selection rides the host-owned
        // native route. No StateTarget/visibility actuation or
        // persist-before-write preimages exist either (the automatic
        // keepAbove is float stacking only), so above stays false.
        assert.equal(caps.windowSuppression, false);
        assert.equal(caps.above, false);
        assert.deepEqual([...caps.unavailableFacts], [...KWIN_UNAVAILABLE_FACTS]);
        const order = [
            "privileged", "managed", "acceptsInput", "activationInhibited", "toolWindow", "owned",
            "transient", "modal", "resizable", "above", "below", "owner", "parent", "role", "appId",
            "executable", "processId", "class", "title", "activationPolicy", "layer",
        ];
        let prev = -1;
        for (const fact of caps.unavailableFacts) {
            const at = order.indexOf(fact);
            assert.ok(at > prev, `${fact} in catalog order, unique`);
            prev = at;
        }
        // processId supported (own token non-null), executable unavailable (own null).
        // resizable stays supported via the script `resizeable` bool
        // (src/window.h:541 at the v6.6.0 floor pin).
        assert.ok(caps.unavailableFacts.indexOf("processId") < 0);
        assert.ok(caps.unavailableFacts.indexOf("executable") >= 0);
        assert.ok(caps.unavailableFacts.indexOf("resizable") < 0);
    });

    it("handshake builder emits the exact v4 shape in contract key order", () => {
        const text = buildHandshakeRequest("adapter-a", "req-1", ownApplicationIdentity(), defaultKwinCapabilities());
        assert.ok(text !== null);
        const body = JSON.parse(text as string) as Record<string, unknown>;
        assert.deepEqual(Object.keys(body), ["type", "v", "adapter", "req", "applicationIdentity", "capabilities"]);
        assert.deepEqual(body["applicationIdentity"], { appId: null, executable: null, processId: OWN_PROCESS_ID_TOKEN });
        const caps = body["capabilities"] as Record<string, unknown>;
        assert.deepEqual(Object.keys(caps), [
            "hostOwned", "equalCount", "birthClear", "nativeSelectorScope", "windowSuppression",
            "maximize", "fullscreen", "nativeSticky", "above", "warp", "tiledFloatTransfer", "unavailableFacts",
        ]);
        // A request never parses as a reply (exact type tag).
        assert.equal(parseHandshakeReply(text as string), null);
        const acceptText = handshakeAccept("adapter-a", "req-1", GEN);
        assert.deepEqual(parseHandshakeReply(acceptText), {
            adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "",
        });
    });

    it("handshake builder fails closed on inconsistency and bad facts", () => {
        const caps = defaultKwinCapabilities();
        // Executable non-null while declared unavailable refuses.
        assert.equal(
            buildHandshakeRequest("adapter-a", "req-1", { appId: null, executable: "x", processId: OWN_PROCESS_ID_TOKEN }, caps),
            null,
        );
        // Null processId while supported refuses.
        assert.equal(
            buildHandshakeRequest("adapter-a", "req-1", { appId: null, executable: null, processId: null }, caps),
            null,
        );
        // Unordered, duplicate, and unknown facts refuse.
        const badFacts: ReadonlyArray<string>[] = [
            ["toolWindow", "privileged"],
            ["privileged", "privileged"],
            ["not-a-fact"],
        ];
        for (const facts of badFacts) {
            assert.equal(
                buildHandshakeRequest("adapter-a", "req-1", ownApplicationIdentity(), { ...caps, unavailableFacts: facts }),
                null,
            );
        }
        assert.equal(buildHandshakeRequest("", "req-1", ownApplicationIdentity(), caps), null);
    });

    it("handshake-reply parses the exact contract shapes and refuses the rest", () => {
        const accept = parseHandshakeReply(handshakeAccept("adapter-a", "req-1", GEN));
        assert.deepEqual(accept, {
            adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "",
        });
        // ready:true with a generation and empty kind is wire-valid per the
        // Rust shape rule (the entry still never binds it: only ready:false
        // accepts), while ready:true without a generation refuses.
        assert.deepEqual(
            parseHandshakeReply(JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: GEN, ready: true, kind: "" })),
            { adapter: "adapter-a", req: "req-1", gen: GEN, ready: true, kind: "" },
        );
        // ready:true, kind on a null-gen refusal, null gen on accept, unknown kind, wrong v/type, extra keys.
        const bad = [
            JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: null, ready: true, kind: "" }),
            JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "malformed" }),
            JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: null, ready: false, kind: "" }),
            JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: null, ready: false, kind: "nope" }),
            JSON.stringify({ type: "handshake-reply", v: 3, adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "" }),
            JSON.stringify({ type: "handshake", v: 4, adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "" }),
            JSON.stringify({ type: "handshake-reply", v: 4, adapter: "adapter-a", req: "req-1", gen: GEN, ready: false, kind: "", extra: 1 }),
        ];
        for (const text of bad) {
            assert.equal(parseHandshakeReply(text), null, `refuses ${text.slice(0, 80)}`);
        }
        // Refusal shape parses.
        assert.deepEqual(
            parseHandshakeReply(JSON.stringify({ type: "handshake-reply", v: 4, adapter: "a", req: "r", gen: null, ready: false, kind: "malformed" })),
            { adapter: "a", req: "r", gen: null, ready: false, kind: "malformed" },
        );
    });

    it("settings builder orders canonical keys and refuses bad revisions", () => {
        const values: Array<[string, unknown]> = [
            ["refill", "last-remaining-workspace"],
            ["gap", 8],
            ["mode", "per-output-local"],
        ];
        const text = buildSettingsRequest("adapter-a", GEN, "set-1", "1", values);
        assert.ok(text !== null);
        const body = JSON.parse(text as string) as Record<string, unknown>;
        assert.deepEqual(Object.keys(body), ["type", "v", "adapter", "gen", "req", "rev", "values"]);
        assert.deepEqual(Object.keys(body["values"] as Record<string, unknown>), ["gap", "mode", "refill"]);
        for (const rev of ["01", "", "0x1", "18446744073709551616"]) {
            assert.equal(buildSettingsRequest("adapter-a", GEN, "set-1", rev, values), null, `bad rev ${rev}`);
        }
        assert.equal(buildSettingsRequest("adapter-a", GEN, "set-1", "1", [["gap", 8], ["gap", 9]]), null);
        assert.equal(buildSettingsRequest("adapter-a", GEN, "set-1", "1", [["gap", Number.NaN]]), null);
    });

    it("settings-reply parses all dispositions and refuses bad shapes", () => {
        for (const status of ["accepted", "stale", "revision-reused"]) {
            const parsed = parseSettingsReply(settingsAccept("adapter-a", GEN, "set-1", "3", status));
            assert.deepEqual(parsed, {
                adapter: "adapter-a", gen: GEN, req: "set-1", rev: "3",
                status, diagnostics: [],
            });
        }
        const withDiag = JSON.parse(settingsAccept("adapter-a", GEN, "set-1", "1")) as Record<string, unknown>;
        withDiag["diagnostics"] = [{ key: "mode", kind: "restart-required" }];
        assert.deepEqual(parseSettingsReply(JSON.stringify(withDiag))?.diagnostics, [
            { key: "mode", kind: "restart-required" },
        ]);
        const bad = [
            JSON.stringify({ type: "settings-reply", v: 4, adapter: "a", gen: GEN, req: "r", rev: "1", status: "nope", diagnostics: [] }),
            JSON.stringify({ type: "settings-reply", v: 4, adapter: "a", gen: GEN, req: "r", rev: "01", status: "accepted", diagnostics: [] }),
            JSON.stringify({ type: "settings-reply", v: 4, adapter: "a", gen: GEN, req: "r", rev: "1", status: "accepted", diagnostics: [{ key: "k", kind: "nope" }] }),
            JSON.stringify({ type: "settings-reply", v: 4, adapter: "a", gen: GEN, req: "r", rev: "1", status: "accepted", diagnostics: [{ key: "k", kind: "missing" }, { key: "k", kind: "missing" }] }),
        ];
        for (const text of bad) {
            assert.equal(parseSettingsReply(text), null);
        }
    });

    it("identity and key bounds are UTF-8 bytes, not JS length", () => {
        // 3000 chars but 6000 bytes: within .length 4096, over 4096 bytes.
        const wide = "\u00e9".repeat(3000);
        assert.equal(
            buildHandshakeRequest("adapter-a", "req-1", { appId: wide, executable: null, processId: OWN_PROCESS_ID_TOKEN }, defaultKwinCapabilities()),
            null,
        );
        assert.equal(
            buildSettingsRequest("adapter-a", GEN, "set-1", "1", [[wide, 8]]),
            null,
            "wide settings key refuses",
        );
    });

    it("connection replies refuse over-bound payloads before JSON.parse", () => {
        const accept = handshakeAccept("adapter-a", "req-1", GEN);
        const pad = " ".repeat(4 * 1024 * 1024);
        // Surrounding whitespace parses as valid JSON, so only the
        // pre-parse bound can refuse these.
        assert.equal(parseHandshakeReply(`${pad}${accept}`), null);
        const settings = settingsAccept("adapter-a", GEN, "set-1", "1");
        assert.equal(parseSettingsReply(`${settings}${pad}`), null);
        assert.equal(parseConnectionError(`${pad}${boundMismatch(GEN)}`), null);
    });
    it("connection errors bind generation-mismatch for the reconnect path", () => {
        assert.deepEqual(parseConnectionError(boundMismatch(GEN)), {
            kind: "generation-mismatch", message: "generation-mismatch", gen: GEN,
        });
        assert.equal(parseConnectionError(JSON.stringify({ v: 4, outcome: "rejected", kind: "x", message: "m", gen: GEN })), null);
    });

    it("native pid maps -1 to the own token, positives to decimal Id text, else null", () => {
        assert.equal(mapNativePid(-1), OWN_PROCESS_ID_TOKEN);
        assert.equal(mapNativePid(1), "1");
        assert.equal(mapNativePid(4194304), "4194304");
        for (const bad of [0, -2, 1.5, Number.NaN, "12", null, undefined, {}, []]) {
            assert.equal(mapNativePid(bad), null);
        }
        assert.ok(isOpaqueId(mapNativePid(-1) as string));
        assert.ok(isOpaqueId(mapNativePid(42) as string));
    });
});

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

function startFlow() {
    const ws = { id: "ws1" };
    const wins = [{
        internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false,
        transientFor: null, dialog: false, modal: false, wantsInput: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: "out" }, desktops: [ws],
        minimized: false, hidden: false, fullscreen: false, maximized: false, sticky: false,
        keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
    }];
    const surface = {
        windowList: () => wins,
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws],
        activeScreen: { name: "out" },
        activeWindow: wins[0],
        currentDesktop: ws,
        cursorPos: { x: 2, y: 100 },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    const calls: Call[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "ReadFloatIntent") {
                let corr = "-";
                try {
                    corr = (JSON.parse(payload) as Record<string, unknown>)["correlation_id"] as string;
                } catch (error) {
                    void error;
                }
                callback(JSON.stringify({
                    v: 2, correlation_id: corr, outcome: "degraded",
                    records: [], stored: 0, returned: 0, reason: "unreadable",
                }));
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
    return { handle: handle as NonNullable<typeof handle>, calls, timers };
}

function discoverCalls(calls: Call[], method: string): Call[] {
    return calls.filter((c) => c.method === method);
}

describe("entry connection sequencing (K1a)", () => {
    it("handshake on DiscoverWorld binds generation and settings gate the first report", () => {
        const flow = startFlow();
        assert.equal(discoverCalls(flow.calls, "DiscoverWorld").length, 1);
        const handshake = discoverCalls(flow.calls, "DiscoverWorld")[0] as Call;
        const hsBody = JSON.parse(handshake.payload) as Record<string, unknown>;
        assert.equal(hsBody["type"], "handshake");
        assert.equal(hsBody["v"], 4);
        assert.equal(hsBody["adapter"], "test-adapter");
        const req = hsBody["req"] as string;
        assert.deepEqual(hsBody["applicationIdentity"], {
            appId: null, executable: null, processId: OWN_PROCESS_ID_TOKEN,
        });
        handshake.callback(handshakeAccept("test-adapter", req, GEN));
        // Settings rides the same method before any DescribeWorld.
        const settingsCalls = discoverCalls(flow.calls, "DiscoverWorld").slice(1);
        assert.equal(settingsCalls.length, 1, "exactly one settings send after handshake");
        const settingsBody = JSON.parse((settingsCalls[0] as Call).payload) as Record<string, unknown>;
        assert.equal(settingsBody["type"], "settings");
        assert.equal(settingsBody["v"], 4);
        assert.equal(settingsBody["gen"], GEN);
        assert.equal(settingsBody["rev"], "1", "fresh lifetime starts at rev 1");
        const values = settingsBody["values"] as Record<string, unknown>;
        assert.ok("gap" in values && "mode" in values && "refill" in values);
        // The first report waits for the accepted settings reply.
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length, 0,
            "no report before accepted settings",
        );
        // A stale settings reply (wrong request) neither settles nor
        // dirties: no report and no resend.
        const stale = JSON.parse(settingsAccept("test-adapter", GEN, "set-999", "1")) as Record<string, unknown>;
        assert.equal(stale["req"], "set-999");
        (settingsCalls[0] as Call).callback(JSON.stringify(stale));
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length, 0,
            "stale settings reply sends no report",
        );
        assert.equal(
            discoverCalls(flow.calls, "DiscoverWorld").length, 2,
            "stale settings reply triggers no resend",
        );
        // The correlated accept makes dispatch authoritative.
        (settingsCalls[0] as Call).callback(
            settingsAccept("test-adapter", GEN, settingsBody["req"] as string, "1"),
        );
        const describeIdx = flow.calls.findIndex((c) => c.method === "DescribeWorld");
        assert.ok(describeIdx >= 0, "report follows accepted settings");
        const capture = flow.calls[describeIdx] as Call;
        assert.equal((JSON.parse(capture.payload) as Record<string, unknown>)["gen"], GEN);
        flow.handle.stop();
    });

    it("lost settings send retries the same revision on the next dispatch", () => {
        const flow = startFlow();
        const handshake = discoverCalls(flow.calls, "DiscoverWorld")[0] as Call;
        handshake.callback(handshakeAccept("test-adapter", (JSON.parse(handshake.payload) as Record<string, unknown>)["req"] as string, GEN));
        const first = discoverCalls(flow.calls, "DiscoverWorld")[1] as Call;
        const firstBody = JSON.parse(first.payload) as Record<string, unknown>;
        const firstRev = firstBody["rev"] as string;
        // Fire the settings 2 s timeout: retry must keep the content key
        // and revision (no bump merely because the timer fired).
        const timeouts = flow.timers.filter((t) => t.delay === 2000);
        assert.ok(timeouts.length >= 1, "settings timeout armed");
        (timeouts[timeouts.length - 1] as { cb: () => void }).cb();
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length, 0,
            "still no report before accepted settings",
        );
        // Any dispatch (here via an explicit focus press) resends settings.
        flow.handle.requestFocus("left");
        const sends = discoverCalls(flow.calls, "DiscoverWorld").slice(1);
        assert.equal(sends.length, 2, "exactly one retry send");
        const retryBody = JSON.parse((sends[1] as Call).payload) as Record<string, unknown>;
        assert.equal(retryBody["type"], "settings");
        assert.equal(retryBody["rev"], firstRev, "equal content retains the revision");
        // Accepting the retry authorizes dispatch, which files the queued
        // focus press via QueueWorldCommand; answering it filed lets the
        // draining capture (the first report) follow.
        (sends[1] as Call).callback(
            settingsAccept("test-adapter", GEN, retryBody["req"] as string, retryBody["rev"] as string),
        );
        const queueCall = flow.calls.filter((c) => c.method === "QueueWorldCommand").pop() as Call | undefined;
        assert.ok(queueCall !== undefined, "queued press files after accepted retry");
        const queueBody = JSON.parse(queueCall.payload) as Record<string, unknown>;
        queueCall.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: GEN,
            client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1",
        }));
        assert.ok(flow.calls.some((c) => c.method === "DescribeWorld"), "report after accepted retry");
        flow.handle.stop();
    });

    it("ready:true handshake never binds (no settings, no report)", () => {
        const flow = startFlow();
        const handshake = discoverCalls(flow.calls, "DiscoverWorld")[0] as Call;
        const req = (JSON.parse(handshake.payload) as Record<string, unknown>)["req"] as string;
        handshake.callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter: "test-adapter", req, gen: GEN, ready: true, kind: "",
        }));
        assert.equal(
            discoverCalls(flow.calls, "DiscoverWorld").length, 1,
            "ready:true binds nothing and sends no settings",
        );
        assert.equal(
            flow.calls.filter((c) => c.method === "DescribeWorld").length, 0,
            "ready:true sends no report",
        );
        flow.handle.stop();
    });

    it("bound host wait deadline only re-arms the wait (no handshake)", () => {
        const flow = startFlow();
        const handshake = discoverCalls(flow.calls, "DiscoverWorld")[0] as Call;
        handshake.callback(handshakeAccept("test-adapter", (JSON.parse(handshake.payload) as Record<string, unknown>)["req"] as string, GEN));
        const settingsCall = discoverCalls(flow.calls, "DiscoverWorld")[1] as Call;
        (settingsCall as Call).callback(
            settingsAccept("test-adapter", GEN, (JSON.parse(settingsCall.payload) as Record<string, unknown>)["req"] as string, "1"),
        );
        const waits = discoverCalls(flow.calls, "WaitWorldWake");
        assert.ok(waits.length >= 1, "wait armed after discovery");
        const discoverCount = discoverCalls(flow.calls, "DiscoverWorld").length;
        const waitCount = waits.length;
        // Two 25 s timers exist post-handshake (the discover host-deadline
        // chain plus the wait watchdog); the watchdog arms later, so target
        // the last one. Firing the stale discover-host timer is a specified
        // no-op (its chain cancelled on success).
        const watchdogs = flow.timers.filter((t) => t.delay === 25000);
        const watchdog = watchdogs[watchdogs.length - 1];
        assert.ok(watchdog !== undefined, "host-deadline watchdog armed");
        (watchdog as { cb: () => void }).cb();
        assert.equal(
            discoverCalls(flow.calls, "DiscoverWorld").length, discoverCount,
            "bound deadline sends no handshake",
        );
        assert.equal(
            discoverCalls(flow.calls, "WaitWorldWake").length, waitCount + 1,
            "bound deadline re-arms the wait",
        );
        flow.handle.stop();
    });

    it("generation-mismatch on the wait lane reconnects with a fresh handshake", () => {
        const flow = startFlow();
        const handshake = discoverCalls(flow.calls, "DiscoverWorld")[0] as Call;
        handshake.callback(handshakeAccept("test-adapter", (JSON.parse(handshake.payload) as Record<string, unknown>)["req"] as string, GEN));
        const settingsCall = discoverCalls(flow.calls, "DiscoverWorld")[1] as Call;
        (settingsCall as Call).callback(
            settingsAccept("test-adapter", GEN, (JSON.parse(settingsCall.payload) as Record<string, unknown>)["req"] as string, "1"),
        );
        const wait = discoverCalls(flow.calls, "WaitWorldWake")[0] as Call;
        const before = discoverCalls(flow.calls, "DiscoverWorld").length;
        wait.callback(boundMismatch(GEN));
        const after = discoverCalls(flow.calls, "DiscoverWorld");
        assert.equal(after.length, before + 1, "mismatch triggers one fresh handshake");
        const hsBody = JSON.parse((after[after.length - 1] as Call).payload) as Record<string, unknown>;
        assert.equal(hsBody["type"], "handshake");
        assert.equal(hsBody["v"], 4);
        flow.handle.stop();
    });
});
