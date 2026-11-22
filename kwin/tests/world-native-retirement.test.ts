// Genuine producer capture -> checked broker -> retained core plan ->
// native runner -> correlated feedback and complete native readback.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";
import { captureWorldObservation, type WorldObservation } from "../src/world-observe";
import {
    buildHandshakeRequest,
    buildSettingsRequest,
    defaultKwinCapabilities,
    ownApplicationIdentity,
} from "../src/world-connection";
import { createWorldSettingsLive } from "../src/world-settings-live";
import {
    buildWorldReportV4InnerJson,
    isCoreWorldReply,
    parseWorldReply,
    type WorldRosterFeedback,
    type WorldRosterPlan,
} from "../src/world-report";
import { createMiniBroker, parseRawEvalWithSettlements, type MiniBroker } from "./world-wire-harness";
import { createWorldApplyHost } from "../src/world-native";
import { applyWorldRoster, createWorldRosterSpendState } from "../src/world-roster";

class WorldBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{ resolve: (r: string) => void; reject: (e: Error) => void }> = [];
    private dead: string | null = null;
    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("raw_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.on("data", () => undefined);
        proc.on("error", (e) => {
            const err = e instanceof Error ? e : new Error(String(e));
            if (this.dead === null) {
                this.dead = err.message;
            }
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(err);
            }
        });
        proc.on("close", (code, signal) => {
            const err = new Error(`raw_world_eval exited ${String(code)}/${String(signal)}`);
            if (this.dead === null) {
                this.dead = err.message;
            }
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(err);
            }
        });
    }
    static start(): WorldBridge {
        return new WorldBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }
    send(method: string, payload: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(JSON.stringify({ method, body: payload }) + "\n");
            } catch (e) {
                reject(e instanceof Error ? e : new Error(String(e)));
            }
        });
    }
    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch {
            void 0;
        }
        await new Promise<void>((resolve) => {
            const t = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch {
                    void 0;
                }
                resolve();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(t);
                resolve();
            });
        });
    }
}

interface Desk {
    id: string;
    name: string;
}
interface Win {
    internalId: string;
    desktops: Desk[];
    output: { name: string };
    onAllDesktops: boolean;
    managed: boolean;
    deleted: boolean;
    minimized: boolean;
    hidden: boolean;
    normalWindow: boolean;
    dialog: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    transient: boolean;
    transientFor: null;
    modal: boolean;
    wantsInput: boolean;
    minSize: null;
    maxSize: null;
    frameGeometry: { x: number; y: number; width: number; height: number };
}

function makeWindow(id: string, desktop: Desk, managed = true): Win {
    return {
        internalId: id,
        desktops: [desktop],
        output: { name: "out-1" },
        onAllDesktops: false,
        managed,
        deleted: false,
        minimized: false,
        hidden: false,
        normalWindow: true,
        dialog: false,
        fullScreen: false,
        maximizeMode: 0,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        minSize: null,
        maxSize: null,
        frameGeometry: { x: 100, y: 100, width: 400, height: 300 },
    };
}

function makeWorld() {
    const desktops: Desk[] = ["d1", "d2", "d3", "d4"].map((id) => ({ id, name: id }));
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const currents = new Map([["out-1", "d4"]]);
    const wins = [makeWindow("win-a", desktops[0] as Desk)];
    const bindings = new Map<string, string>();
    const removed: string[] = [];
    const surface = {
        windowList: () => wins as unknown[],
        screens,
        desktops,
        activeScreen: { name: "out-1" },
        activeWindow: null as unknown,
        cursorPos: { x: 2, y: 100 },
        windowAt: () => [],
        stackingOrder: [],
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => {
            const id = currents.get(screen.name) ?? "";
            return desktops.find((d) => d.id === id) ?? null;
        },
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            currents.set(screen.name, (deskRef as Desk).id);
        },
        removeDesktop: (desktop: Desk) => {
            removed.push(desktop.id);
            const index = desktops.indexOf(desktop);
            if (index >= 0) desktops.splice(index, 1);
        },
    };
    return { desktops, screens, currents, wins, bindings, removed, surface };
}

type W = ReturnType<typeof makeWorld>;

function observe(w: W): WorldObservation | null {
    return captureWorldObservation({
        nativePerOutput: true,
        workspace: w.surface as never,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId: string) => {
            const logical = w.bindings.get(nativeId);
            if (logical === undefined) {
                return null;
            }
            return { logical, out: "out-1", pin: false, disp: false, managed: true };
        },
    });
}

async function connectBridge(
    bridge: WorldBridge,
    broker: MiniBroker,
): Promise<string> {
    // Authentic v4 connection pump: the handshake binds the producer
    // adapter lifetime and mints the serving generation; the revisioned
    // settings snapshot (rev 1) precedes the first report on the wire.
    // Production builders only: the own identity plus the default
    // per-output KDE capabilities (host-owned natives, never a generic
    // Windows-like hostOwned:false/unavailableFacts:[] pair the strict
    // core refuses as snapshot-invalid against native-shaped facts), and
    // the genuine default settings snapshot the production entry sends.
    const hsBody = buildHandshakeRequest("test-adapter", "req-1", ownApplicationIdentity(), defaultKwinCapabilities(true));
    assert.ok(hsBody !== null, "production handshake request builds");
    const hsReplyText = await bridge.send("DiscoverWorld", hsBody);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
    assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
    assert.equal(hsReply["kind"], "", "accepted handshake carries an empty kind");
    assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
    assert.equal(hsReply["req"], "req-1", "handshake echoes the request correlation");
    assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", "handshake mints the serving generation");
    const gen = hsReply["gen"] as string;
    const boundHs = broker.bindDiscovery(hsReplyText);
    assert.equal((JSON.parse(boundHs) as Record<string, unknown>)["adapter"], "test-adapter", "harness passes the producer lifetime through");
    assert.equal(broker.gen, gen, "harness records the actual serving generation");
    // Genuine default settings exactly as the production entry sends them:
    // same readers and seed values (per-output-local mode, default-tiled,
    // 8px gaps), snapshotted through the production settings module.
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
    return gen;
}

async function roundtrip(
    bridge: WorldBridge,
    broker: MiniBroker,
    w: W,
    src: string,
    gen: string,
    feedback?: WorldRosterFeedback,
): Promise<string> {
    const obs = observe(w);
    assert.ok(obs !== null, "capture stays non-null");
    assert.equal(obs.complete, true, "capture complete");
    const inner = buildWorldReportV4InnerJson({
        focusChanges: null,
        observation: obs,
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        stateAttempts: [],
        intentRecords: [],
        ...(feedback === undefined ? {} : { rosterFeedback: feedback }),
    });
    assert.ok(inner !== null, "genuine v4 producer bytes encode");
    const envelope = JSON.stringify({ v: 4, adapter: "test-adapter", gen, source: src, nativeInvalid: "0", report: JSON.parse(inner) });
    const stamped = broker.stampCapture(envelope);
    const { reply: coreRaw, settlements } = parseRawEvalWithSettlements(await bridge.send("DescribeWorldWithSettlements", stamped));
    const bound = broker.bindReply(coreRaw, envelope, settlements);
    const parsed = JSON.parse(bound) as { outcome?: unknown };
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `core accepts, got ${bound.slice(0, 400)}`);
    // Converge adapter bindings from the accepted reply for the next capture.
    const body = JSON.parse(bound) as {
        workspaces: Array<{ id: string; out: string; bind: string }>;
    };
    w.bindings.clear();
    for (const row of body.workspaces) {
        if (row.out === "out-1" && row.bind !== "") {
            w.bindings.set(row.bind, row.id);
        }
    }
    return bound;
}

function replyOf(bound: string) {
    const reply = parseWorldReply(bound);
    assert.ok(reply !== null && isCoreWorldReply(reply), "bound reply parses as core");
    return reply;
}

function removalPlan(bound: string, survivors: string[], removals: string[]): WorldRosterPlan {
    const reply = replyOf(bound);
    const plan = reply.rosterPlan;
    assert.ok(plan !== undefined, "broker publishes a native slot plan");
    assert.deepEqual(plan.baseline.order, ["d1", "d2", "d3", "d4"]);
    assert.deepEqual(plan.slots, survivors.map((native) => ({ kind: "existing", native })));
    assert.deepEqual(plan.removals.map((step) => step.native), removals);
    assert.deepEqual(plan.members, []);
    assert.deepEqual(plan.views, []);
    assert.equal(plan.focus, null);
    assert.deepEqual(reply.nativeCreations, [], "no legacy spare creation");
    return plan;
}

function nativeRunner(w: W, opts: { surface?: object; complete?: boolean; protected?: ReadonlySet<string> } = {}) {
    const native = createWorldApplyHost(opts.surface ?? w.surface, (value, prop) => Reflect.get(value, prop));
    const spend = createWorldRosterSpendState();
    return (plan: WorldRosterPlan) => applyWorldRoster(plan, {
        fresh: () => true,
        read: () => opts.complete === false ? null : native.readRosterWitness(),
        canRemove: (id) => opts.protected?.has(id) !== true && native.resolveDesktop(id) !== null,
        canWrite: (out) => native.resolveOutput(out) !== null,
        canAffect: (win) => {
            const ref = native.resolveRosterWindow(win);
            return ref !== null && native.rosterMemberWritable(ref);
        },
        memberHome: (win) => {
            const ref = native.resolveRosterWindow(win);
            return ref === null ? null : native.readRosterHome(ref);
        },
        append: () => assert.fail("shrink may not append"),
        move: () => assert.fail("shrink may not move"),
        select: () => assert.fail("shrink may not switch views"),
        focus: () => assert.fail("shrink may not focus"),
        remove: native.removeDesktop,
    }, spend);
}

async function captureMiddleRemoval(bridge: WorldBridge, broker: MiniBroker, w: W) {
    const gen = await connectBridge(bridge, broker);
    await roundtrip(bridge, broker, w, "src-1", gen);
    const bound = await roundtrip(bridge, broker, w, "src-2", gen);
    return { gen, plan: removalPlan(bound, ["d1", "d4"], ["d2", "d3"]) };
}

describe("world native retirement", () => {
    it("removes only authorized middle slots and settles absence with ordered survivors and no replay", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const w = makeWorld();
            const { gen, plan } = await captureMiddleRemoval(bridge, broker, w);
            const run = nativeRunner(w);
            const result = run(plan);
            assert.equal(result.complete, true, `clean removal stopped at ${result.stopped}`);
            assert.deepEqual(w.removed, ["d2", "d3"]);
            assert.deepEqual(w.desktops.map((d) => d.id), ["d1", "d4"], "host stops at its floor without reordering");
            assert.equal(w.currents.get("out-1"), "d4");
            assert.deepEqual(w.wins[0]?.desktops.map((d) => d.id), ["d1"]);
            assert.ok(result.feedback !== null);
            assert.deepEqual(result.feedback.steps.map((step) => step.step), plan.removals.map((step) => step.step));
            assert.deepEqual(result.feedback.steps.map(({ status, before, after }) => [status, before, after]), [
                ["applied", { kind: "order", order: ["d1", "d2", "d3", "d4"] }, { kind: "order", order: ["d1", "d3", "d4"] }],
                ["applied", { kind: "order", order: ["d1", "d3", "d4"] }, { kind: "order", order: ["d1", "d4"] }],
            ]);
            const settled = replyOf(await roundtrip(bridge, broker, w, "src-3", gen, result.feedback));
            assert.equal(settled.rosterPlan, undefined, "complete native absence settles the operation");
            const rows = settled.workspaces.filter((row) => row.out === "out-1");
            assert.deepEqual(rows.map((r) => r.bind), ["d1", "d4"]);
            const baseline = plan.baseline.projections.find((row) => row.out === "out-1")?.rows;
            assert.ok(baseline !== undefined);
            assert.deepEqual(rows.map((r) => r.id), [baseline[0], baseline[3]], "ordered survivor IDs are unchanged");
            assert.deepEqual(rows.map((r) => r.order), [0, 1]);
            assert.deepEqual(settled.nativeCreations, [], "absence does not recreate retired natives");
            assert.equal(run(plan).complete, true, "replayed operation consumes no step again");
            assert.equal(replyOf(await roundtrip(bridge, broker, w, "src-4", gen)).rosterPlan, undefined);
            assert.deepEqual(w.removed, ["d2", "d3"]);
        } finally {
            await bridge.close();
        }
    });

    it("managed sticky keeps its retained home while the empty middle still retires", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const w = makeWorld();
            const sticky = makeWindow("win-s", w.desktops[1] as Desk);
            // Keep the middle occupied initially, so shrink is first issued
            // after the sticky transition and the other middle becomes empty.
            w.wins.push(sticky, makeWindow("win-leaves", w.desktops[2] as Desk));
            const gen = await connectBridge(bridge, broker);
            const initial = replyOf(await roundtrip(bridge, broker, w, "src-1", gen));
            const home = initial.workspaces.find((row) => row.out === "out-1" && row.bind === "d2")?.id;
            assert.ok(home, "managed window starts at a real logical home on d2");
            sticky.onAllDesktops = true;
            sticky.desktops = [];
            w.wins.pop();
            const bound = await roundtrip(bridge, broker, w, "src-2", gen);
            const plan = removalPlan(bound, ["d1", "d2", "d4"], ["d3"]);
            const result = nativeRunner(w)(plan);
            assert.equal(result.complete, true, `sticky-safe removal stopped at ${result.stopped}`);
            assert.ok(result.feedback !== null);
            assert.deepEqual(w.removed, ["d3"]);
            assert.deepEqual(w.desktops.map((d) => d.id), ["d1", "d2", "d4"]);
            assert.deepEqual(sticky.desktops, [], "sticky membership was not rewritten");
            assert.equal(sticky.onAllDesktops, true);
            const settled = replyOf(await roundtrip(bridge, broker, w, "src-3", gen, result.feedback));
            assert.equal(settled.rosterPlan, undefined);
            assert.equal(settled.workspaces.find((row) => row.id === home)?.bind, "d2", "sticky home keeps its native");
        } finally {
            await bridge.close();
        }
    });

    it("fresh occupant, visibility, protection and incomplete evidence veto a captured removal", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const w = makeWorld();
            const { gen, plan } = await captureMiddleRemoval(bridge, broker, w);
            for (const [label, change, opts] of [
                ["unmanaged occupant", (live: W) => live.wins.push(makeWindow("fresh", live.desktops[1] as Desk, false)), {}],
                ["visible", (live: W) => live.currents.set("out-1", "d2"), {}],
                ["protected", () => undefined, { protected: new Set(["d2", "d3"]) }],
                ["incomplete", () => undefined, { complete: false }],
            ] as const) {
                const live = makeWorld();
                change(live);
                const result = nativeRunner(live, opts)(plan);
                assert.equal(result.complete, false, label);
                assert.equal(result.feedback, null, label);
                assert.deepEqual(live.removed, [], label);
                assert.deepEqual(live.desktops.map((d) => d.id), plan.baseline.order, label);
            }
            const pending = replyOf(await roundtrip(bridge, broker, w, "src-3", gen)).rosterPlan;
            assert.deepEqual(pending, plan, "held operation does not become a new authorization");
        } finally {
            await bridge.close();
        }
    });

    it("production host malformed occupants, unknown selectors and fresh floor hold captured removals", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const { plan } = await captureMiddleRemoval(bridge, broker, makeWorld());
            const occupant = { internalId: "fresh", output: { name: "out-1" }, managed: false };
            for (const window of [
                { ...occupant, onAllDesktops: false, desktops: [] },
                { ...occupant, onAllDesktops: false, desktops: [null] },
                { ...occupant, onAllDesktops: false, desktops: [{ id: "" }] },
                { ...occupant, onAllDesktops: "unknown", desktops: [{ id: "d1" }] },
                { ...occupant, onAllDesktops: false, desktops: [{ id: "d2" }, { id: "d3" }] },
                { ...occupant, onAllDesktops: false, get desktops(): never { throw new Error("unreadable"); } },
            ]) {
                const live = makeWorld();
                const surface = { ...live.surface, windowList: () => [...live.wins, window] };
                const result = nativeRunner(live, { surface })(plan);
                assert.equal(result.complete, false, "unknown occupancy or a real subset occupant vetoes");
                assert.equal(result.feedback, null);
                assert.deepEqual(live.removed, []);
            }
            {
                const live = makeWorld();
                live.screens.push({ name: "out-unknown", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
                assert.equal(nativeRunner(live)(plan).complete, false);
                assert.deepEqual(live.removed, [], "unknown current on any live output holds");
            }
            {
                const live = makeWorld();
                live.desktops.splice(2, 2);
                live.currents.set("out-1", "d1");
                assert.equal(nativeRunner(live)(plan).complete, false);
                assert.deepEqual(live.removed, [], "external shrink to the KDE floor cannot be followed by a stale removal");
                assert.deepEqual(live.desktops.map((d) => d.id), ["d1", "d2"], "pending candidate remains present at the floor");
            }
            {
                const live = makeWorld();
                const surface = { ...live.surface, removeDesktop: (desktop: Desk) => live.removed.push(desktop.id) };
                const run = nativeRunner(live, { surface });
                const result = run(plan);
                assert.equal(result.complete, false, "accepted setter does not prove absence");
                assert.deepEqual(live.desktops.map((d) => d.id), plan.baseline.order);
                assert.equal(run(plan).complete, false, "replay does not manufacture absence");
                assert.deepEqual(live.removed, ["d2"], "deferred removal neither permits the next step nor replays");
            }
        } finally {
            await bridge.close();
        }
    });
});

describe("production retirement fresh evidence", () => {
    for (const [label, window] of [
        ["empty-list non-sticky", { onAllDesktops: false, desktops: [] }],
        ["malformed member", { onAllDesktops: false, desktops: [null] }],
        ["unreadable sticky", { onAllDesktops: null, desktops: [{ id: "d1" }] }],
    ] as const) {
        it(label, () => {
            const host = createWorldApplyHost({ windowList: () => [window] }, (v, p) => Reflect.get(v, p));
            const count = host.desktopOccupants("d2");
            assert.ok(!count.readable || count.total > 0, "never prove empty");
        });
    }
    for (const [label, window] of [
        ["sticky flag", { onAllDesktops: true, desktops: [{ id: "d1" }] }],
        ["empty-list sticky", { onAllDesktops: true, desktops: [] }],
    ] as const) {
        it(`${label} proves empty`, () => {
            const host = createWorldApplyHost({ windowList: () => [window] }, (v, p) => Reflect.get(v, p));
            assert.deepEqual(host.desktopOccupants("d2"), { total: 0, readable: true });
        });
    }
    it("one readable current cannot hide another unreadable output", () => {
        const host = createWorldApplyHost({
            screens: [{ name: "a" }, { name: "b" }],
            currentDesktopForScreen: (screen: { name: string }) => screen.name === "a" ? { id: "d1" } : null,
        }, (v, p) => Reflect.get(v, p));
        assert.equal(host.desktopVisible("d2"), null);
        assert.equal(host.desktopVisible("d1"), true);
        const reversed = createWorldApplyHost({
            screens: [{ name: "b" }, { name: "a" }],
            currentDesktopForScreen: (screen: { name: string }) => screen.name === "a" ? { id: "d1" } : null,
        }, (v, p) => Reflect.get(v, p));
        assert.equal(reversed.desktopVisible("d2"), null, "later readable output cannot erase unknown");
        assert.equal(reversed.desktopVisible("d1"), true, "positive visibility wins over unknown");
    });
});
