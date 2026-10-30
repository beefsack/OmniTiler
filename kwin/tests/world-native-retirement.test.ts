// Native retirement producer->core->apply->readback (offline, real v3 capture
// + real core via raw_world_eval + mechanical apply). Adopted empty unviewed
// middle desktops publish existence:false while rows/bindings persist;
// mechanical apply removes only explicit authorized natives; the next
// complete capture without the native clears the binding and deletes or
// validly recycles the row with stable survivors and compact ordinals.
// Fresh occupant/visibility/pin/floor vetoes after the captured desire hold
// removal; logical spares never mint creations. Fails if the core
// retirement helper is disabled (all existence:true).

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";
import { captureWorldObservation, type WorldObservation } from "../src/world-observe";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import {
    buildWorldInnerReportJson,
    isCoreWorldReply,
    parseWorldReply,
} from "../src/world-report";
import { createMiniBroker } from "./world-wire-harness";
import { createWorldApplyHost, type WorldApplyHost } from "../src/world-native";

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
                env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
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

function makeWorld() {
    const desktops: Desk[] = ["d1", "d2", "d3", "d4"].map((id) => ({ id, name: id }));
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const currents = new Map([["out-1", "d1"]]);
    const wins: Win[] = [];
    const bindings = new Map<string, string>();
    const removed: string[] = [];
    const surface = {
        windowList: () => wins as unknown[],
        screens,
        desktops,
        activeScreen: { name: "out-1" },
        activeWindow: null as unknown,
        cursorPos: { x: 2, y: 100 },
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

async function roundtrip(
    bridge: WorldBridge,
    broker: ReturnType<typeof createMiniBroker>,
    w: W,
    src: string,
): Promise<{ obs: WorldObservation; bound: string }> {
    const obs = observe(w);
    assert.ok(obs !== null, "capture stays non-null");
    assert.equal(obs.complete, true, "capture complete");
    const inner = buildWorldInnerReportJson({
        observation: obs,
        defaultTiled: true,
        refill: "last-remaining-workspace",
        bornMaximized: "tile-on-launch",
        anchorFallback: "closest-splittable-relative",
        noSpaceResult: "allow-overlap",
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        clearAttempts: [],
    });
    const envelope = JSON.stringify({ report: JSON.parse(inner), source: src, nativeInvalid: "0" });
    const stamped = broker.stampCapture(envelope);
    const coreRaw = await bridge.send("DescribeWorld", stamped);
    const bound = broker.bindReply(coreRaw, envelope);
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
    return { obs, bound };
}

function workspacesOf(bound: string): Array<{ id: string; out: string; bind: string; exist: boolean; order: number }> {
    const body = JSON.parse(bound) as {
        workspaces: Array<{ id: string; out: string; bind: string; exist: boolean; order: number }>;
    };
    return body.workspaces.filter((r) => r.out === "out-1");
}

function applyWith(
    obs: WorldObservation,
    bound: string,
    opts: {
        occupants?: (nativeId: string) => { total: number; readable: boolean };
        visible?: (nativeId: string) => boolean | null;
        removed: string[];
        minDesktops?: number;
        complete?: boolean;
        pinned?: Set<string>;
        wins?: Win[];
        currents?: Map<string, string>;
        desktops?: Desk[];
        nativeHost?: WorldApplyHost;
    },
): ReturnType<typeof applyWorldReply> {
    const parsed = parseWorldReply(bound);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "bound reply parses as core");
    const state = new WorldAdapterState("test-adapter", "owner-a");
    assert.equal(state.onDiscovery(parsed.gen, "broker-1"), true);
    const flight = state.beginCapture(parsed.source, null);
    assert.ok(flight !== null);
    const verdict = state.acceptReply(parsed, parsed.source, (flight as { watermark: string }).watermark);
    assert.deepEqual(verdict, { applied: true, reason: "accepted" });
    const pinned = opts.pinned ?? new Set<string>();
    const observation: WorldObservation =
        opts.complete === false ? { ...obs, complete: false } : pinned.size === 0 ? obs : {
            ...obs,
            bindings: [...obs.bindings, ...[...pinned].map((native) => ({ logical: "pinned", native, out: "out-1", pin: true, disp: false, managed: true }))],
        };
    const env: ApplyEnv = {
        state,
        observation,
        reply: parsed,
        dispatchWatermark: state.currentInvalid,
        takeResult: null,
        log: () => undefined,
        isStillListed: () => true,
        isActiveWindow: () => false,
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: () => ({ out: "out-1", ws: "d1" }),
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        setGeometry: () => true,
        setMembership: () => true,
        sendToOutput: () => true,
        resolveOutput: () => ({}),
        resolveDesktop: () => ({}),
        switchView: () => true,
        createDesktop: () => undefined,
        removeDesktop: (nativeId: string) => {
            opts.removed.push(nativeId);
            const list = opts.desktops;
            if (list !== undefined) {
                const at = list.findIndex((d) => d.id === nativeId);
                if (at >= 0) {
                    list.splice(at, 1);
                }
            }
            return true;
        },
        focusWindow: () => true,
        desktopOccupants: opts.occupants ?? (() => ({ total: 0, readable: true })),
        desktopVisible: opts.visible ??
            ((nativeId: string) => {
                const cur = opts.currents?.get("out-1") ?? "d1";
                return nativeId === cur;
            }),
        readLiveView: (out: string) => opts.currents?.get(out) ?? "d1",
        desktopById: (id: string) => {
            const list = opts.desktops ?? [];
            return (list.find((d) => d.id === id) ?? null) as unknown as object | null;
        },
        ...(opts.nativeHost ?? {}),
        minDesktops: opts.minDesktops ?? 2,
    };
    return applyWorldReply(env);
}

describe("world native retirement", () => {
    it("adopted empty middle retires, apply removes only authorized, absence settles with stable survivors", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const w = makeWorld();
            const disc = await bridge.send("DiscoverWorld", JSON.stringify({ adapter: "test-adapter", req: "req-1" }));
            broker.bindDiscovery(disc);
            await roundtrip(bridge, broker, w, "src-1");
            const second = await roundtrip(bridge, broker, w, "src-2");
            const rows = workspacesOf(second.bound);
            assert.equal(rows.length, 4, `four rows persist, got ${JSON.stringify(rows)}`);
            const byBind = new Map(rows.map((r) => [r.bind, r]));
            // d1 viewed, d4 trailing spare: stay. Middle empties retire.
            assert.equal(byBind.get("d1")?.exist, true, `view vetoes: ${second.bound.slice(0, 500)}`);
            assert.equal(byBind.get("d4")?.exist, true, `trailing vetoes: ${second.bound.slice(0, 500)}`);
            assert.equal(byBind.get("d2")?.exist, false, `empty middle retires: ${second.bound.slice(0, 500)}`);
            assert.equal(byBind.get("d3")?.exist, false, `empty middle retires: ${second.bound.slice(0, 500)}`);
            // Discriminates the new core eligibility: without it every row
            // would read existence:true.
            assert.ok(rows.some((r) => r.exist === false), "at least one retirement authorized");
            const parsed = JSON.parse(second.bound) as { nativeCreations: unknown[] };
            assert.deepEqual(parsed.nativeCreations, [], "logical spare mints no creation");

            // Mechanical apply removes only the explicit authorized natives.
            const removed = w.removed;
            const result = applyWith(second.obs, second.bound, {
                removed,
                currents: w.currents,
                desktops: w.desktops,
                nativeHost: createWorldApplyHost(w.surface, (v, p) => Reflect.get(v, p)),
            });
            assert.ok(removed.includes("d2") && removed.includes("d3"), `both middles removed, got ${JSON.stringify(removed)}`);
            assert.ok(!removed.includes("d1") && !removed.includes("d4"), `view/trailing never removed, got ${JSON.stringify(removed)}`);
            assert.equal(result.removals, removed.length);

            // Next complete capture without the natives clears bindings and
            // settles rows with stable survivors and compact ordinals.
            const third = await roundtrip(bridge, broker, w, "src-3");
            const next = workspacesOf(third.bound);
            assert.ok(!next.some((r) => r.bind === "d2" || r.bind === "d3"), `bindings cleared: ${JSON.stringify(next)}`);
            const l1 = byBind.get("d1")?.id;
            const l4 = byBind.get("d4")?.id;
            const ids = next.map((r) => r.id);
            assert.ok(ids.includes(l1 as string) && ids.includes(l4 as string), `survivors stable: ${JSON.stringify(next)}`);
            next.forEach((r, index) => assert.equal(r.order, index, `compact ordinals: ${JSON.stringify(r)}`));
            const thirdParsed = JSON.parse(third.bound) as { nativeCreations: unknown[] };
            assert.deepEqual(thirdParsed.nativeCreations, [], "absence never recreates");
        } finally {
            await bridge.close();
        }
    });

    it("fresh occupant, visibility, pin, floor, and incomplete vetoes hold removal", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const w = makeWorld();
            const disc = await bridge.send("DiscoverWorld", JSON.stringify({ adapter: "test-adapter", req: "req-1" }));
            broker.bindDiscovery(disc);
            await roundtrip(bridge, broker, w, "src-1");
            const second = await roundtrip(bridge, broker, w, "src-2");
            const rows = workspacesOf(second.bound);
            assert.ok(rows.some((r) => r.exist === false), "desire captured before vetoes");

            // Fresh unmanaged occupant race on both candidates.
            {
                const removed: string[] = [];
                const result = applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    occupants: () => ({ total: 1, readable: true }),
                });
                assert.equal(removed.length, 0, "occupant race holds every removal");
                assert.ok(result.held > 0);
            }
            // Unreadable occupancy holds too (never blind removal).
            {
                const removed: string[] = [];
                const result = applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    occupants: () => ({ total: 0, readable: false }),
                });
                assert.equal(removed.length, 0, "unknown occupancy holds");
                assert.ok(result.held > 0);
            }
            // Fresh visibility race on the candidates.
            {
                const removed: string[] = [];
                const result = applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    visible: () => true,
                });
                assert.equal(removed.length, 0, "visible race holds");
                assert.ok(result.held > 0);
            }
            // Fresh pin on the candidates.
            {
                const removed: string[] = [];
                applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    pinned: new Set(["d2", "d3"]),
                });
                assert.ok(!removed.includes("d2") && !removed.includes("d3"), `pinned natives kept, got ${JSON.stringify(removed)}`);
            }
            // Minimum floor: live count at the floor authorizes nothing.
            {
                const removed: string[] = [];
                const result = applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    minDesktops: 4,
                });
                assert.equal(removed.length, 0, "floor holds");
                assert.ok(result.held > 0);
            }
            // Incomplete observation never authorizes blind removal.
            {
                const removed: string[] = [];
                applyWith(second.obs, second.bound, {
                    removed,
                    currents: w.currents,
                    desktops: w.desktops,
                    complete: false,
                });
                assert.equal(removed.length, 0, "incomplete capture holds");
            }
            // Row plus binding survive held vetoes until complete evidence:
            // the same desire still publishes with rows and bindings intact.
            const again = await roundtrip(bridge, broker, w, "src-3");
            const againRows = workspacesOf(again.bound);
            assert.equal(againRows.length, 4, "rows persist through held vetoes");
            assert.equal(againRows.filter((r) => r.bind !== "").length, 4, "bindings persist through held vetoes");
        } finally {
            await bridge.close();
        }
    });

    it("production host fresh sticky, malformed, visibility and floor reads veto captured desires", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const captured = makeWorld();
            broker.bindDiscovery(await bridge.send("DiscoverWorld", JSON.stringify({ adapter: "test-adapter", req: "req-1" })));
            await roundtrip(bridge, broker, captured, "src-1");
            const second = await roundtrip(bridge, broker, captured, "src-2");
            for (const windows of [
                [{ onAllDesktops: true, desktops: [{ id: "d1" }] }],
                [{ onAllDesktops: false, desktops: [] }],
                [{ onAllDesktops: false, desktops: [null] }],
                [{ onAllDesktops: false, desktops: [{ id: "" }] }],
                [{ onAllDesktops: "unknown", desktops: [{ id: "d1" }] }],
                [{ onAllDesktops: false, desktops: [{ id: "d2" }, { id: "d3" }] }],
                [{ get desktops(): never { throw new Error("unreadable"); } }],
            ]) {
                const live = makeWorld();
                const host = createWorldApplyHost({ ...live.surface, windowList: () => windows }, (v, p) => Reflect.get(v, p));
                const result = applyWith(second.obs, second.bound, { removed: live.removed, nativeHost: host });
                assert.equal(result.removals, 0, "fresh all-native window evidence holds removal");
                assert.deepEqual(live.removed, []);
            }
            {
                const live = makeWorld();
                live.screens.push({ name: "out-unknown", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
                const host = createWorldApplyHost(live.surface, (v, p) => Reflect.get(v, p));
                assert.equal(applyWith(second.obs, second.bound, { removed: live.removed, nativeHost: host }).removals, 0);
                assert.deepEqual(live.removed, [], "unknown current on any live output holds");
            }
            {
                const live = makeWorld();
                const host = createWorldApplyHost(live.surface, (v, p) => Reflect.get(v, p));
                assert.equal(applyWith(second.obs, second.bound, { removed: live.removed, nativeHost: host, minDesktops: 3 }).removals, 1);
                assert.equal(live.desktops.length, 3, "floor re-read limits multiple removals");
            }
            {
                const removed: string[] = [];
                assert.equal(applyWith(second.obs, second.bound, { removed, minDesktops: 3 }).removals, 1);
                assert.equal(removed.length, 1, "snapshot-only host also consumes the floor budget");
            }
            {
                const live = makeWorld();
                live.desktops.splice(2, 1);
                const host = createWorldApplyHost(live.surface, (v, p) => Reflect.get(v, p));
                assert.equal(applyWith(second.obs, second.bound, { removed: live.removed, nativeHost: host, minDesktops: 3 }).removals, 0);
                assert.equal(live.desktops.length, 3, "external removal after capture cannot defeat fresh floor");
            }
            {
                const live = makeWorld();
                const host = createWorldApplyHost({ ...live.surface, removeDesktop: (desktop: Desk) => live.removed.push(desktop.id) }, (v, p) => Reflect.get(v, p));
                assert.equal(applyWith(second.obs, second.bound, { removed: live.removed, nativeHost: host, minDesktops: 3 }).removals, 1);
                assert.equal(live.removed.length, 1, "accepted but deferred removal still consumes floor budget");
            }
        } finally {
            await bridge.close();
        }
    });
});

describe("production retirement fresh evidence", () => {
    for (const [label, window] of [
        ["sticky flag", { onAllDesktops: true, desktops: [{ id: "d1" }] }],
        ["empty-list sticky", { onAllDesktops: false, desktops: [] }],
        ["malformed member", { onAllDesktops: false, desktops: [null] }],
        ["unreadable sticky", { onAllDesktops: null, desktops: [{ id: "d1" }] }],
    ] as const) {
        it(label, () => {
            const host = createWorldApplyHost({ windowList: () => [window] }, (v, p) => Reflect.get(v, p));
            const count = host.desktopOccupants("d2");
            assert.ok(!count.readable || count.total > 0, "never prove empty");
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
