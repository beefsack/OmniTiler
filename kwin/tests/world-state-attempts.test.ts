// Raw native-state attempt lane (v4 StateAttempt): closed fullscreen/
// maximized/sticky/above typing, strict canonical encoding, retained
// delivery/ack plumbing, and strict-codec acceptance. Offline only:
// in-memory state plus genuine producer bytes through the retained real
// `WorldEngine` (`raw_world_eval`). This unit retains evidence only: no
// native state spending happens here (no fullscreen/maximized/sticky/
// above writes), so the apply loop records nothing yet. Included in
// `npm test`.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";

import { captureWorldObservation } from "../src/world-observe";
import {
    buildCaptureEnvelope,
    buildWorldReportV4InnerJson,
    encodeV4StateAttempt,
    type WorldStateAttempt,
    type WorldStateProperty,
} from "../src/world-report";
import { WorldAdapterState } from "../src/world-state";
import {
    buildHandshakeRequest,
    buildSettingsRequest,
    defaultKwinCapabilities,
    ownApplicationIdentity,
    parseHandshakeReply,
    parseSettingsReply,
} from "../src/world-connection";
import { createWorldSettingsLive } from "../src/world-settings-live";
import { isCanonicalU64 } from "../src/world-seq";
import { createMiniBroker } from "./world-wire-harness";

// Proven minimal producer fakes (same shape as the v4 report producer
// test): native KWin property spellings, no D-Bus, no KWin.
function makeDesk(id: string) {
    return { id };
}

function makeWin(over: Record<string, unknown>) {
    return {
        internalId: "win-a",
        normalWindow: true,
        dialog: false,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        frameGeometry: { x: 8.4, y: 8.6, width: 800, height: 600 },
        clientGeometry: { x: 10, y: 30, width: 796, height: 578 },
        minSize: { width: 100, height: 100 },
        maxSize: { width: 2147483647, height: 2147483647 },
        output: { name: "out-a" },
        desktops: [makeDesk("native-a")],
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        ...over,
    };
}

function makeSurface(wins: Array<Record<string, unknown>>) {
    const desktops = [makeDesk("native-a")];
    const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    return {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: { name: "out-a" },
        activeWindow: wins[0] ?? null,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => (wins.length > 0 ? [wins[0]] : []),
        stackingOrder: [...wins].reverse(),
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
}

function observe(surface: object) {
    const obs = captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-a", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
    assert.ok(obs !== null);
    return obs;
}

function stateRow(over: Partial<WorldStateAttempt> & { win: string }): WorldStateAttempt {
    return {
        property: "fullscreen",
        target: true,
        rev: "2",
        setter: "accepted",
        pre: false,
        post: true,
        attemptMs: "7",
        ...over,
    };
}

describe("world state attempt encoding", () => {
    it("spells all four properties in exact canonical key order", () => {
        const props: WorldStateProperty[] = ["fullscreen", "maximized", "sticky", "above"];
        for (const property of props) {
            const bytes = encodeV4StateAttempt(stateRow({ win: "win-a", property }));
            assert.equal(
                bytes,
                `{"win":"win-a","property":"${property}","target":true,` +
                `"rev":"2","setter":"accepted","pre":false,"post":true,"attemptMs":"7"}`,
            );
            assert.deepEqual(Object.keys(JSON.parse(bytes as string)), [
                "win", "property", "target", "rev", "setter", "pre", "post", "attemptMs",
            ]);
        }
    });
    it("spells every bool/null pre/post combination without defaults", () => {
        const flag = (v: boolean | null): string => (v === null ? "null" : (v ? "true" : "false"));
        for (const pre of [true, false, null] as const) {
            for (const post of [true, false, null] as const) {
                for (const target of [true, false] as const) {
                    const bytes = encodeV4StateAttempt(stateRow({ win: "win-a", target, pre, post }));
                    assert.equal(
                        bytes,
                        `{"win":"win-a","property":"fullscreen","target":${flag(target)},` +
                        `"rev":"2","setter":"accepted","pre":${flag(pre)},"post":${flag(post)},"attemptMs":"7"}`,
                        `exact bytes for target=${flag(target)} pre=${flag(pre)} post=${flag(post)}`,
                    );
                }
            }
        }
        // Canonical max counters ride verbatim (u64 max, never wrapped).
        const maxed = encodeV4StateAttempt(stateRow({
            win: "win-a", rev: "18446744073709551615", attemptMs: "18446744073709551615",
        }));
        assert.equal(
            maxed,
            `{"win":"win-a","property":"fullscreen","target":true,` +
            `"rev":"18446744073709551615","setter":"accepted","pre":false,"post":true,` +
            `"attemptMs":"18446744073709551615"}`,
        );
        // Target is never nullable: null refuses.
        assert.equal(
            encodeV4StateAttempt({ ...stateRow({ win: "win-a" }), target: null as never }),
            null,
        );
    });
    it("refuses invalid values, ids, counters, and setters", () => {
        const base = stateRow({ win: "win-a" });
        const bad: unknown[] = [
            null, [], "x", 7,
            { ...base, win: "" },
            { ...base, win: "bad id" },
            { ...base, win: null },
            { ...base, property: "minimized" },
            { ...base, property: "Fullscreen" },
            { ...base, property: "" },
            { ...base, target: 1 },
            { ...base, target: "true" },
            { ...base, rev: "" },
            { ...base, rev: "01" },
            { ...base, rev: "x" },
            { ...base, rev: "18446744073709551616" },
            { ...base, setter: "not-called" },
            { ...base, setter: "" },
            { ...base, setter: "ACCEPTED" },
            { ...base, pre: 0 },
            { ...base, pre: "false" },
            { ...base, pre: [] },
            { ...base, post: 1 },
            { ...base, post: "true" },
            { ...base, attemptMs: "" },
            { ...base, attemptMs: "-1" },
            { ...base, attemptMs: "0x1" },
            { ...base, attemptMs: "7.0" },
        ];
        for (const row of bad) {
            assert.equal(encodeV4StateAttempt(row as WorldStateAttempt), null, `refuses ${JSON.stringify(row)}`);
            assert.equal(
                new WorldAdapterState("adapter-a", "owner-a").recordStateAttempt(row as WorldStateAttempt),
                false,
                `never stores ${JSON.stringify(row)}`,
            );
        }
        assert.deepEqual(new WorldAdapterState("adapter-a", "owner-a").statesForReport(), []);
    });
});

describe("world state attempt retention", () => {
    it("keeps latest-wins per window property across all setters", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        for (const setter of ["accepted", "refused", "threw"] as const) {
            assert.equal(state.recordStateAttempt(
                stateRow({ win: "win-a", property: "maximized", target: true, rev: "2", setter, pre: null, post: null }),
            ), true);
        }
        // Same-tuple replays (identical window/property/target/rev) keep
        // the first row: the original stamp is never replaced.
        assert.equal(state.recordStateAttempt(
            stateRow({ win: "win-a", property: "maximized", target: true, rev: "2", setter: "threw", pre: true, post: true, attemptMs: "9" }),
        ), true);
        const rows = state.statesForReport();
        assert.equal(rows.length, 1);
        assert.deepEqual(rows[0], stateRow({ win: "win-a", property: "maximized", target: true, rev: "2", setter: "accepted", pre: null, post: null }));
        // A newer tuple for the same property (new target and rev)
        // replaces its predecessor: the core settles one fact per
        // property and refuses two rows as duplicate-state-attempt.
        assert.equal(state.recordStateAttempt(
            stateRow({ win: "win-a", property: "maximized", target: false, rev: "3", setter: "refused", pre: true, post: true, attemptMs: "8" }),
        ), true);
        const next = state.statesForReport();
        assert.equal(next.length, 1);
        assert.deepEqual(next[0], stateRow({ win: "win-a", property: "maximized", target: false, rev: "3", setter: "refused", pre: true, post: true, attemptMs: "8" }));
    });
    it("retains unrelated properties and windows side by side, sorted", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-b", property: "above" })), true);
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", property: "sticky" })), true);
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", property: "fullscreen" })), true);
        assert.deepEqual(
            state.statesForReport().map((r) => `${r.win}/${r.property}`),
            ["win-a/fullscreen", "win-a/sticky", "win-b/above"],
        );
    });
    it("evicts oldest-first past 1024 rows", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        for (let i = 0; i < 1025; i += 1) {
            assert.equal(state.recordStateAttempt(
                stateRow({ win: `win-${String(i).padStart(4, "0")}`, property: "above" }),
            ), true);
        }
        const rows = state.statesForReport();
        assert.equal(rows.length, 1024);
        assert.ok(!rows.some((r) => r.win === "win-0000"), "oldest row evicted");
        assert.ok(rows.some((r) => r.win === "win-1024"), "newest row retained");
    });
    it("clones snapshots so later mutation cannot rewrite delivery", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        const rows = state.statesForReport();
        (rows[0] as WorldStateAttempt).rev = "999";
        (rows[0] as WorldStateAttempt).target = false;
        assert.deepEqual(state.statesForReport()[0], stateRow({ win: "win-a" }));
    });
    it("clears every retained row on generation reset", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", property: "sticky" })), true);
        state.onDiscovery("gen-2", "adapter-a");
        assert.deepEqual(state.statesForReport(), []);
    });
});

describe("world state attempt acknowledgement", () => {
    const reply = (rev: string, obs: string, src = "src-1"): never => ({
        adapter: "adapter-a", gen: "gen-1", req: src, source: src,
        ready: true, consumed: false, rev, obs, clock: "9",
    } as never);
    it("redelivers the exact snapshot with original stamps after delivery loss", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        const first = state.beginCapture("src-1", null);
        assert.ok(first !== null);
        assert.deepEqual(first.delivered.states, [stateRow({ win: "win-a" })]);
        // Transport loss drops the flight but retains every row.
        state.noteRejectedOrTransportError();
        assert.equal(state.inFlight, null);
        assert.deepEqual(state.statesForReport(), [stateRow({ win: "win-a" })]);
        const second = state.beginCapture("src-2", null);
        assert.ok(second !== null);
        assert.deepEqual(second.delivered.states, [stateRow({ win: "win-a" })], "redelivery reuses the original stamp");
    });
    it("retires exactly-equal rows while newer and unrelated rows survive", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", property: "sticky" })), true);
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        // A newer tuple for the same property replaces the delivered
        // predecessor before its ack lands.
        assert.equal(state.recordStateAttempt(
            stateRow({ win: "win-a", target: false, rev: "3", setter: "refused", pre: true, post: true, attemptMs: "8" }),
        ), true);
        // An unrelated window joins after the snapshot.
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-b", property: "above" })), true);
        const retired = state.ackDelivered("src-1", true);
        assert.equal(retired, 1, "only the exactly-equal sticky row retires");
        assert.deepEqual(
            state.statesForReport(),
            [
                stateRow({ win: "win-a", target: false, rev: "3", setter: "refused", pre: true, post: true, attemptMs: "8" }),
                stateRow({ win: "win-b", property: "above" }),
            ],
        );
    });
    it("consumes nothing on unready, foreign-source, malformed, or stale replies", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        assert.equal(state.ackDelivered("src-1", false), 0);
        assert.equal(state.ackDelivered("src-other", true), 0);
        assert.deepEqual(state.statesForReport(), [stateRow({ win: "win-a" })]);
        assert.deepEqual(state.acceptReply(reply("x", "1"), "src-1", "0"), { applied: false, reason: "malformed-counter" });
        assert.ok(state.inFlight !== null, "malformed reply keeps the flight");
        assert.deepEqual(state.statesForReport(), [stateRow({ win: "win-a" })], "malformed reply retains the row");
        assert.deepEqual(state.acceptReply(reply("2", "2"), "src-1", "0"), { applied: true, reason: "accepted" });
        assert.deepEqual(state.statesForReport(), [], "valid reply consumes the exact snapshot");
        assert.ok(state.beginCapture("src-2", null) !== null);
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", rev: "3" })), true);
        assert.deepEqual(state.acceptReply(reply("1", "1", "src-2"), "src-2", "0"), { applied: false, reason: "stale-rev" });
        assert.deepEqual(state.statesForReport(), [stateRow({ win: "win-a", rev: "3" })], "stale reply retains the newer row");
    });
});

describe("world state attempt report lane", () => {
    function buildInner(states: WorldStateAttempt[]) {
        const obs = observe(makeSurface([makeWin({})]));
        return buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
            stateAttempts: states,
            intentRecords: [],
        });
    }
    it("always presents the required lane, empty or populated", () => {
        const empty = buildInner([]);
        assert.ok(empty !== null);
        assert.deepEqual((JSON.parse(empty as string) as Record<string, unknown>)["stateAttempts"], []);
        const rows = [
            stateRow({ win: "win-a", property: "fullscreen" }),
            stateRow({ win: "win-a", property: "sticky", target: false, rev: "3", setter: "threw", pre: null, post: false, attemptMs: "8" }),
        ];
        const full = buildInner(rows);
        assert.ok(full !== null);
        assert.deepEqual((JSON.parse(full as string) as Record<string, unknown>)["stateAttempts"], rows);
    });
    it("fails a malformed row closed without poisoning retained evidence", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        assert.equal(state.recordStateAttempt(stateRow({ win: "win-a" })), true);
        assert.equal(state.recordStateAttempt({ ...stateRow({ win: "win-a" }), property: "minimized" as never }), false);
        assert.deepEqual(state.statesForReport(), [stateRow({ win: "win-a" })], "refused row never stores");
        const poisoned = buildInner([{ ...stateRow({ win: "win-a" }), rev: "01" }]);
        assert.equal(poisoned, null, "malformed row fails the whole report closed");
        const clean = buildInner(state.statesForReport());
        assert.ok(clean !== null, "retained evidence still builds after a refused row");
        assert.deepEqual((JSON.parse(clean as string) as Record<string, unknown>)["stateAttempts"], [stateRow({ win: "win-a" })]);
    });
});

describe("world state attempt strict engine", () => {
    function repoRoot(): string {
        let dir = resolve(process.cwd());
        for (let depth = 0; depth < 4; depth += 1) {
            if (existsSync(join(dir, "Cargo.toml"))) {
                return dir;
            }
            dir = dirname(dir);
        }
        throw new Error("fixture root not found");
    }
    class WorldBridge {
        private readonly proc: ChildProcess;
        private readonly waiters: Array<{ resolve: (reply: string) => void; reject: (error: Error) => void }> = [];
        private dead: string | null = null;
        private stderrTail = "";
        private constructor(proc: ChildProcess) {
            this.proc = proc;
            const stdout = proc.stdout;
            if (stdout === null || stdout === undefined) {
                throw new Error("raw_world_eval stdout unavailable");
            }
            createInterface({ input: stdout }).on("line", (line: string) => {
                this.waiters.shift()?.resolve(line);
            });
            proc.stderr?.on("data", (chunk: unknown) => {
                this.stderrTail = (this.stderrTail + String(chunk)).slice(-8192);
            });
            proc.on("error", (error) => this.failAll(error instanceof Error ? error : new Error(String(error))));
            proc.on("close", (code, signal) => {
                this.failAll(new Error(`raw_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
            });
        }
        private failAll(error: Error): void {
            if (this.dead === null) {
                this.dead = error.message;
            }
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(error);
            }
        }
        static start(): WorldBridge {
            return new WorldBridge(
                spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
                    cwd: repoRoot(),
                    stdio: ["pipe", "pipe", "pipe"],
                }),
            );
        }
        send(method: string, payload: string): Promise<string> {
            if (this.dead !== null) {
                return Promise.reject(new Error(this.dead));
            }
            if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
                return Promise.reject(new Error("raw_world_eval process already exited"));
            }
            return new Promise<string>((resolvePromise, rejectPromise) => {
                this.waiters.push({ resolve: resolvePromise, reject: rejectPromise });
                try {
                    this.proc.stdin?.write(JSON.stringify({ method, body: payload }) + "\n");
                } catch (error) {
                    rejectPromise(error instanceof Error ? error : new Error(String(error)));
                }
            });
        }
        async close(): Promise<void> {
            try {
                this.proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        this.proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                this.proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
            });
        }
    }
    it("accepts genuine producer state-attempt bytes at decode level only (no settlement proven)", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            // Test-only capability: the above lane rides below, so claim
            // it on the handshake to avoid an above attempt conflicting
            // with the handshake.
            const handshake = buildHandshakeRequest("test-adapter", "req-hs", ownApplicationIdentity(), { ...defaultKwinCapabilities(true), above: true });
            assert.ok(handshake !== null, "handshake builds");
            const hsReplyText = await bridge.send("DiscoverWorld", handshake);
            const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply", `engine answers the handshake, got ${hsReplyText.slice(0, 200)}`);
            assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
            assert.equal(hsReply["kind"], "", "accepted handshake carries an empty kind");
            assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
            assert.equal(hsReply["req"], "req-hs", "handshake echoes the request correlation");
            const gen = hsReply["gen"] as string;
            assert.ok(typeof gen === "string" && gen !== "", "handshake mints the serving generation");
            assert.ok(parseHandshakeReply(hsReplyText) !== null, "sent handshake accept parses strict");
            broker.bindDiscovery(hsReplyText);
            // Revisioned settings snapshot precedes the first report on
            // the wire: genuine defaults exactly as the production entry
            // sends them (per-output-local mode, default-tiled, 8px gaps).
            const liveSettings = createWorldSettingsLive({
                readWorkspaceModeFn: () => "per-output-local",
                readDefaultTiledFn: () => true,
                readInnerGapFn: () => 8,
                readOuterGapFn: () => 8,
            });
            const setBody = buildSettingsRequest("test-adapter", gen, "settings-1", "1", liveSettings.snapshotConnectionValues());
            assert.ok(setBody !== null, "production settings request builds");
            const setReplyText = await bridge.send("DiscoverWorld", setBody);
            const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
            assert.equal(setReply["type"], "settings-reply", "engine answers settings");
            assert.equal(setReply["status"], "accepted", `settings accepted, got ${setReplyText.slice(0, 300)}`);
            assert.equal(setReply["adapter"], "test-adapter", "settings reply retains the producer adapter");
            assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
            assert.equal(setReply["req"], "settings-1", "settings reply echoes the request correlation");
            assert.equal(setReply["rev"], "1", "settings reply binds the sent revision");
            assert.ok(parseSettingsReply(setReplyText) !== null, "sent settings accept parses strict");
            // Genuine producer bytes: the production capture plus the
            // production canonical builder, one row per property (the most
            // the strict codec admits per window).
            const state = new WorldAdapterState("test-adapter", "owner-a");
            const props: WorldStateProperty[] = ["fullscreen", "maximized", "sticky", "above"];
            for (const property of props) {
                assert.equal(state.recordStateAttempt(stateRow({ win: "win-a", property })), true);
            }
            const obs = observe(makeSurface([makeWin({})]));
            const inner = buildWorldReportV4InnerJson({
                observation: obs,
                creationAttempts: [],
                frameAttempts: [],
                memberAttempts: [],
                viewAttempts: [],
                clearAttempts: [],
                stateAttempts: state.statesForReport(),
                intentRecords: [],
            });
            assert.ok(inner !== null, "producer bytes build");
            const envelope = buildCaptureEnvelope("test-adapter", gen, "src-1", "1", "1", inner as string);
            const stamped = broker.stampCapture(envelope);
            const stampedValue = JSON.parse(stamped) as Record<string, unknown>;
            assert.equal((stampedValue["stateAttempts"] as unknown[]).length, 4, "all four property rows ride verbatim");
            const coreReply = await bridge.send("DescribeWorld", stamped);
            const parsed = JSON.parse(coreReply) as Record<string, unknown>;
            // Accepted strict shape is the absence of the error envelope:
            // encode_error carries outcome/kind/message/gen while the
            // accepted encode_reply_v4 carries
            // v/adapter/gen/req/obs/rev/ready/consumed plus the desire
            // lanes. Scope ends at decode-level acceptance: the injected
            // revs ("2") match no core-issued state target, so rows may
            // hold unsettled and no settlement is proven here.
            assert.ok(!("outcome" in parsed), `real strict engine accepts the state-attempt capture, got ${coreReply.slice(0, 400)}`);
            assert.equal(parsed["v"], 4, "accepted reply is v4");
            assert.equal(parsed["adapter"], "test-adapter", "accepted reply retains the producer adapter");
            assert.equal(parsed["gen"], gen, "accepted reply binds the serving generation");
            assert.equal(parsed["req"], "src-1", "accepted reply binds the capture source");
            assert.equal(parsed["ready"], true, "accepted capture carries ready:true");
            assert.equal(parsed["consumed"], false, "observation-only capture consumes nothing");
            assert.ok(isCanonicalU64(parsed["obs"]), "accepted reply carries a canonical obs");
            assert.ok(isCanonicalU64(parsed["rev"]), "accepted reply carries a canonical rev");
        } finally {
            await bridge.close();
        }
    });
});
