// Command-only directional coverage through the production world entry:
// floating/sticky subjects route the same Planner focus/move-directional
// commands as tiled subjects (the core owns the float lattice and
// half-snap geometry). No native writes; failures close with one bounded
// line. Real production commands; no Planner core import.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

// Opaque v2 intent record bytes: the exact core-minted schema1 float bytes
// established in world-entry.test.ts (proven by the service roundtrip plus
// the explicit float/tile restore coverage), carried inside schema2
// records here. The entry never interprets them.
const INTENT_FLOAT_DATA = "{\"v\":1,\"float\":true,\"stickyPrior\":null}";

interface FakeDesktop {
    id: string;
}
interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    // Truthful undecorated client rect: the fake carries no decoration, so
    // the client mirrors the frame exactly (delta 0).
    clientGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    setMaximize: (vertically: boolean, horizontally: boolean) => void;
    minSize: null;
    maxSize: null;
    resourceClass?: string;
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const desk = (id: string): FakeDesktop => ({ id });

function stickyWin(id: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    const fake: FakeWindow = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 100, width: 200, height: 200 },
        clientGeometry: { x, y: 100, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: true,
        sticky: true,
        keepAbove: false,
        keepBelow: false,
        setMaximize: (vertically: boolean, horizontally: boolean): void => {
            fake.maximizeMode = vertically && horizontally ? 3 : 0;
        },
        minSize: null,
        maxSize: null,
    };
    return fake;
}

// Ordinary (non-sticky) float: not on all desktops, floated only through
// the persisted opaque intent record (v2 ReadFloatIntent) live-joined on
// the first report.
function floatWin(id: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    const fake = stickyWin(id, out, ws, x);
    fake.onAllDesktops = false;
    fake.sticky = false;
    return fake;
}

interface Harness {
    world: {
        screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
        desktops: FakeDesktop[];
        wins: FakeWindow[];
        activeScreen: { name: string };
        activeWindow: FakeWindow | null;
        currentByScreen: Map<string, FakeDesktop>;
        focused: string[];
        geometryWrites: string[];
    };
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    shortcuts: Map<string, () => void>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: () => void;
}

function makeHarness(
    wins: FakeWindow[],
    screens: Harness["world"]["screens"],
    desktops: FakeDesktop[],
    currentByScreen: Map<string, FakeDesktop>,
    opts?: { scopeClass?: string[]; floatIntentMembers?: string[] },
): Harness {
    const world = {
        screens,
        desktops,
        wins,
        activeScreen: { name: screens[0]?.name ?? "out-a" },
        activeWindow: wins[0] ?? null,
        currentByScreen,
        focused: [] as string[],
        geometryWrites: [] as string[],
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const shortcuts = new Map<string, () => void>();
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
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.focused.push(found.internalId);
            }
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentByScreen.set(screen.name, found);
            }
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null,
        removeDesktop: () => false,
    };
    for (const w of world.wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                // Keep the truthful undecorated client tracking the frame
                // so no capture pairs a fresh frame with a stale client.
                w.clientGeometry = { ...rect };
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
    }
    const intentRecords = (opts?.floatIntentMembers ?? []).map((win) => ({ win, data: INTENT_FLOAT_DATA }));
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "ReadFloatIntent") {
                const corr = (JSON.parse(payload) as Record<string, unknown>)["correlation_id"] as string;
                callback(JSON.stringify({
                    v: 2, correlation_id: corr, outcome: "ok",
                    records: intentRecords, stored: intentRecords.length, returned: intentRecords.length,
                }));
                return;
            }
            if (method === "WriteFloatIntent") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const corr = body["correlation_id"] as string;
                const stored = (body["records"] as Array<unknown>).length;
                callback(JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored }));
                return;
            }
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const takeBody = JSON.parse(payload) as Record<string, unknown>;
                const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                const failed = (globalThis as Record<string, unknown>)["__takeFailed"] as { kind: string; message: string } | undefined;
                if (failed !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeFailed"];
                    callback(JSON.stringify({ v: 4, outcome: "failed", gen: takeGen, globalCmd: "1", kind: failed.kind, message: failed.message }));
                    return;
                }
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                }
            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: (name, _text, _seq, cb) => {
            shortcuts.set(name, cb);
            return true;
        },
        readWorkspaceModeFn: () => "per-output-local",
        ...(opts?.scopeClass !== undefined ? { scopeClass: opts.scopeClass } : {}),
        ...(opts?.floatIntentMembers !== undefined ? { floatIntent: true as const } : {}),
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            // Debounce/drain only (0/120ms): never fire the 2000ms flight
            // and take timeouts, or a reply below would land after the
            // timeout settled the flight. Host-deadline timers (>=25000,
            // see WAIT_HOST_DEADLINE_MS) never fire in these offline flows.
            if (t.delay > 120) {
                continue;
            }
            timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    };
    return { world, calls, timers, logs, shortcuts, handle, runTimers };
}

function answerHandshake(h: Harness, gen = "gen-1"): void {
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
    const req = (JSON.parse(found.payload) as Record<string, unknown>)["req"] as string;
    found.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: Harness, gen = "gen-1"): void {
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
    const body = JSON.parse(found.payload) as Record<string, unknown>;
    found.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
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
        scopes: [{ scope: { kind: "workspace", out: "out-a", ws: "ws-a" }, auth: "controlled", reason: "" }],
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

function discover(h: Harness, gen = "gen-1"): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    // The opaque intent seed hydrates before the first admissible report:
    // the v2 read auto-answers synchronously in callDbus above; pump once
    // more when persistence is enabled so hydration settles before the
    // first capture.
    const read = [...h.calls].reverse().find((c) => c.method === "ReadFloatIntent");
    if (read !== undefined) {
        h.runTimers();
    }
    // Settle the discovery-triggered observation report first: the entry
    // auto-dispatches one observation-only DescribeWorld on bind, and a
    // command issued while it is in flight queues in FIFO order behind it.
    // Replying here leaves the flight idle for the test body.
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined, "expected DescribeWorld");
    const autoBody = JSON.parse(auto.payload);
    assert.equal(autoBody["command"], undefined, "captures never carry an inline command");
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(acceptedReply(gen, autoBody.source, autoBody.nativeInvalid, "0"));
}

function queuePayloads(h: Harness): Array<Record<string, unknown>> {
    return h.calls.filter((c) => c.method === "QueueWorldCommand").map((c) => JSON.parse(c.payload) as Record<string, unknown>);
}

describe("command-only directional intents for float subjects", () => {
    it("routes sticky focus as a direction-only command with no native writes", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500), stickyWin("win-g", "out-a", ws, 700)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.world.focused.length = 0;
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, [], "float focus never writes natively");
        assert.deepEqual(h.world.geometryWrites, []);
        h.runTimers();
        const focusOps = queuePayloads(h)
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "focus-directional");
        assert.ok(focusOps.length >= 1);
        const inner = focusOps[focusOps.length - 1] as Record<string, unknown>;
        assert.equal(inner["win"], "win-f");
        assert.equal(inner["direction"], "right");
        assert.ok(!("floatSubject" in inner) && !("out" in inner), "no adapter routing key on the wire");
    });

    it("drives the shortcut through the same direction-only focus command", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500), stickyWin("win-g", "out-a", ws, 700)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        const cb = h.shortcuts.get("omnitiler-focus-right");
        assert.ok(cb !== undefined);
        h.world.focused.length = 0;
        cb();
        assert.deepEqual(h.world.focused, [], "shortcut focus never writes natively");
        h.runTimers();
        const focusOps = queuePayloads(h)
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "focus-directional");
        assert.ok(focusOps.length >= 1);
    });

    it("routes sticky moves as a direction-only command with no native geometry writes", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.handle?.requestMove("right");
        assert.deepEqual(h.world.geometryWrites, [], "float moves never write geometry natively");
        assert.deepEqual(h.world.focused, [], "float moves never write focus natively");
        h.runTimers();
        const moveOps = queuePayloads(h)
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "move-directional");
        assert.ok(moveOps.length >= 1);
        const inner = moveOps[moveOps.length - 1] as Record<string, unknown>;
        assert.equal(inner["win"], "win-f");
        assert.equal(inner["direction"], "right");
        assert.ok(!("out" in inner) && !("crossOutput" in inner));
    });

    it("fails closed with no command on a missing live subject", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.world.activeWindow = null;
        h.handle?.requestFocus("right");
        h.runTimers();
        assert.equal(queuePayloads(h).length, 0, "missing subject files nothing");
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed action=focus")));
    });

    it("queues directional presses in order behind flight instead of refusing as busy", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.handle?.requestFocus("left");
        h.handle?.requestMove("right");
        // The head files on the idle fast path while the tail waits behind
        // the flight: no busy refusal, no native writes. No timer flush
        // here: flushing the debounce first would file a commandless
        // observe and strand the head flight.
        let ops = queuePayloads(h).map((p) => (p["command"] as Record<string, unknown>)["op"]);
        assert.ok(ops.includes("focus-directional"), `head files first, got ${JSON.stringify(ops)}`);
        assert.ok(!h.logs.some((l) => l.includes("command-busy")), "directionals queue, never refuse as busy");
        assert.deepEqual(h.world.geometryWrites, []);
        assert.deepEqual(h.world.focused, []);
        // Settle the head flight; the tail then files in order.
        const head = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(head !== undefined);
        const headBody = JSON.parse(head.payload) as Record<string, unknown>;
        head.callback(acceptedReply("gen-1", headBody["source"] as string, headBody["nativeInvalid"] as string, "2"));
        h.runTimers();
        ops = queuePayloads(h).map((p) => (p["command"] as Record<string, unknown>)["op"]);
        assert.ok(ops.includes("focus-directional") && ops.includes("move-directional"), `FIFO drains both, got ${JSON.stringify(ops)}`);
    });

    it("fails closed with no writes when undiscovered (pre-discovery refusal)", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        // No discovery: the serving generation is unbound, so every command
        // refuses before the wire with native state untouched.
        h.handle?.requestMove("right");
        h.runTimers();
        assert.equal(queuePayloads(h).length, 0);
        assert.deepEqual(h.world.geometryWrites, []);
        assert.deepEqual(h.world.focused, []);
        assert.ok(h.logs.some((l) => l.includes("command-refused")));
    });

    it("selects with genuine neutral intent (null output)", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const selectOps = queuePayloads(h)
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "select-relative");
        assert.ok(selectOps.length >= 1);
        assert.equal((selectOps[selectOps.length - 1] as Record<string, unknown>)["out"], null);
    });

    it("routes an ordinary float (intent membership, not sticky) as commands with no native writes", () => {
        const ws = desk("ws-a");
        const wins = [floatWin("win-f", "out-a", ws, 500), floatWin("win-g", "out-a", ws, 700)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]), { floatIntentMembers: ["win-f", "win-g"] });
        discover(h);
        // Truthful transport: the persisted v2 intent store seeded both
        // floats before the first dispatch, so the subject is an ordinary
        // float; the seed rides the first report live-joined with exact
        // bytes (never locally interpreted).
        assert.ok(h.calls.some((c) => c.method === "ReadFloatIntent"), "intent membership seeded over D-Bus");
        const firstReport = JSON.parse(h.calls.filter((c) => c.method === "DescribeWorld")[0]?.payload as string) as Record<string, unknown>;
        const seeded = ((firstReport["report"] as Record<string, unknown>)["intentRecords"] as Array<Record<string, unknown>>)
            .map((r) => ({ win: r["win"], data: r["data"] }))
            .sort((a, b) => String(a.win).localeCompare(String(b.win)));
        assert.deepEqual(seeded, [
            { win: "win-f", data: INTENT_FLOAT_DATA },
            { win: "win-g", data: INTENT_FLOAT_DATA },
        ]);
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, [], "ordinary-float focus never writes natively");
        assert.deepEqual(h.world.geometryWrites, []);
        const move = h.shortcuts.get("omnitiler-move-right");
        assert.ok(move !== undefined, "directional move shortcut registered");
        move();
        assert.deepEqual(h.world.geometryWrites, [], "ordinary-float shortcut move never writes geometry natively");
        assert.deepEqual(h.world.focused, [], "ordinary-float shortcut move never writes focus natively");
        // Flush the seed/discovery debounce so the focus files while the
        // shortcut move waits behind the flight; settle the head, then the
        // tail files. No accept lands before this flush, so no flight can
        // be stranded by a commandless observe.
        h.runTimers();
        const head = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(head !== undefined);
        const headBody = JSON.parse(head.payload) as Record<string, unknown>;
        head.callback(acceptedReply("gen-1", headBody["source"] as string, headBody["nativeInvalid"] as string, "2"));
        h.runTimers();
        const ops = queuePayloads(h).map((p) => p["command"] as Record<string, unknown>);
        assert.ok(ops.some((c) => c["op"] === "focus-directional" && c["win"] === "win-f" && c["direction"] === "right"));
        assert.ok(ops.some((c) => c["op"] === "move-directional" && c["win"] === "win-f" && c["direction"] === "right"));
        for (const c of ops) {
            assert.ok(!("floatSubject" in c) && !("out" in c) && !("crossOutput" in c), "no adapter routing key on the wire");
        }
    });

    it("covers every neutral selection verb and both numbered/relative sends", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        const acceptLast = (): void => {
            const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
            assert.ok(last !== undefined);
            const body = JSON.parse(last.payload) as Record<string, unknown>;
            // Each acceptance settles its own flight: advance the scripted
            // revision together with obs so the accepted-rev gate admits
            // every leg (never a stale repeat).
            acceptRev += 1;
            last.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, String(acceptRev)));
            h.runTimers();
        };
        const lastCommand = (seen: number): Record<string, unknown> => {
            const payloads = queuePayloads(h);
            assert.ok(payloads.length > seen, `command filed, got ${JSON.stringify(payloads.map((p) => p["command"]))}`);
            return payloads[seen]?.["command"] as Record<string, unknown>;
        };
        let seen = queuePayloads(h).length;
        let acceptRev = 1;
        h.handle?.requestWorkspaceSelect(0);
        h.runTimers();
        assert.deepEqual(lastCommand(seen), { op: "select-trailing", out: null });
        acceptLast();
        seen = queuePayloads(h).length;
        h.handle?.requestWorkspaceSelect(3);
        h.runTimers();
        assert.deepEqual(lastCommand(seen), { op: "select-numbered", out: null, index: 3 });
        acceptLast();
        seen = queuePayloads(h).length;
        h.handle?.requestWorkspacePrevious();
        h.runTimers();
        assert.deepEqual(lastCommand(seen), { op: "select-previous", out: null });
        acceptLast();
        seen = queuePayloads(h).length;
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        assert.deepEqual(lastCommand(seen), { op: "select-relative", out: null, delta: -1 });
        acceptLast();
        seen = queuePayloads(h).length;
        h.handle?.requestWorkspaceRelativeMove(1, true);
        h.runTimers();
        assert.deepEqual(lastCommand(seen), {
            op: "send-relative",
            win: "win-f",
            source: { implicit: "home-then-action" },
            delta: 1,
            follow: true,
        });
        acceptLast();
        seen = queuePayloads(h).length;
        (h.handle as unknown as { requestWorkspaceMove: (index: number, follow: boolean) => void }).requestWorkspaceMove(0, false);
        h.runTimers();
        assert.deepEqual(lastCommand(seen), {
            op: "send-numbered",
            win: "win-f",
            source: { implicit: "home-then-action" },
            index: 0,
            follow: false,
        });
        assert.deepEqual(h.world.geometryWrites, [], "selection/send verbs never write natively");
        assert.deepEqual(h.world.focused, [], "selection/send verbs never write focus natively");
    });

    it("refuses a foreign live subject under a scope class with no command", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]), { scopeClass: ["allowed-class"] });
        discover(h);
        // The live window carries no matching resourceClass, so the scoped
        // entry reads no active subject: the command never names it.
        h.handle?.requestFocus("right");
        h.handle?.requestMove("right");
        h.runTimers();
        assert.equal(queuePayloads(h).length, 0, "foreign subject files nothing");
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed action=focus")));
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed action=move")));
        assert.deepEqual(h.world.geometryWrites, []);
        assert.deepEqual(h.world.focused, []);
    });

    it("surfaces a terminal service refusal with no native writes", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        (globalThis as Record<string, unknown>)["__takeFailed"] = { kind: "refused-target", message: "core-refused" };
        h.handle?.requestFocus("right");
        assert.ok(queuePayloads(h).some(
            (p) => (p["command"] as Record<string, unknown>)["op"] === "focus-directional",
        ), "command files before the core refuses it");
        // Accept the command-carrying flight itself (no timer flush first:
        // the debounce would file a commandless observe and strand it),
        // so the entry collects the staged terminal core refusal.
        const flight = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(flight !== undefined);
        const body = JSON.parse(flight.payload) as Record<string, unknown>;
        flight.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, "2"));
        h.runTimers();
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:command-failed kind=refused-target")), "terminal core refusal surfaces");
        assert.deepEqual(h.world.geometryWrites, []);
        assert.deepEqual(h.world.focused, []);
    });
});
