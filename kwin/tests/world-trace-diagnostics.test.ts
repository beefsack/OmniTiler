// Bounded trace-only float/state-target diagnostics: every rapid-toggle
// press records its press basis plus clientCmd and submitted marker, and
// every accepted apply records the per-window core plan, observation facts
// and the newly recorded frame/state attempts. Aggregate `applied`
// geometry counts prove only that something moved; these lines locate the
// first per-window divergence instead. Trace-gated only: no production log
// change.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { DISABLED_PRESENTATION } from "./world-wire-harness";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { NATIVE_PREIMAGE_VERSION } from "../src/world-native-preimage";

const WS = "ws1";
const OUT = "out";
const WIN_A = "win-a";
const WIN_B = "win-b";
const TILE_A: [number, number, number, number] = [0, 0, 400, 600];
const TILE_B: [number, number, number, number] = [400, 0, 400, 600];
const FLOAT_A: [number, number, number, number] = [162, 122, 475, 355];

interface FakeWin {
    internalId: string; managed: boolean; deleted: boolean; transient: boolean;
    transientFor: unknown; dialog: boolean; modal: boolean; wantsInput: boolean;
    normalWindow: boolean;
    clientGeometry: { x: number; y: number; width: number; height: number } | null;
    output: { name: string }; desktops: Array<{ id: string }>;
    minimized: boolean; hidden: boolean;
    fullscreen: boolean; maximized: boolean; sticky: boolean;
    keepAbove: boolean; keepBelow: boolean;
    minSize: null; maxSize: null;
    fullScreen: boolean; maximizeMode: number; onAllDesktops: boolean;
    resourceClass: string;
    setMaximize: (v: boolean, h: boolean) => void;
}

function makeWin(id: string, frame: { x: number; y: number; width: number; height: number }): FakeWin {
    const w = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null,
        dialog: false, modal: false, wantsInput: true, normalWindow: true,
        frameGeometry: { ...frame },
        clientGeometry: { ...frame },
        output: { name: OUT }, desktops: [{ id: WS }], minimized: false, hidden: false,
        fullscreen: false, maximized: false, sticky: false,
        keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        fullScreen: false, maximizeMode: 0, onAllDesktops: false, resourceClass: "allowed",
        setMaximize: undefined as unknown as (v: boolean, h: boolean) => void,
    } as unknown as FakeWin & { frameGeometry: { x: number; y: number; width: number; height: number } };
    w.setMaximize = (v: boolean, h: boolean): void => {
        w.maximizeMode = v && h ? 3 : v ? 1 : h ? 2 : 0;
    };
    return w;
}

function fakeSignal(): { handlers: Array<(...a: Array<unknown>) => void>; signal: { connect: (h: (...a: Array<unknown>) => void) => void; disconnect: (h: (...a: Array<unknown>) => void) => void } } {
    const handlers: Array<(...a: Array<unknown>) => void> = [];
    return { handlers, signal: { connect: (h) => { handlers.push(h); }, disconnect: (h) => { const i = handlers.indexOf(h); if (i >= 0) handlers.splice(i, 1); } } };
}

interface Harness {
    wins: FakeWin[]; activeId: string;
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void; answered: boolean }>;
    timers: Array<{ delay: number; cb: () => void }>; logs: string[];
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    runTimers: (maxDelay?: number) => void;
    setActive: (id: string | null) => void;
    // Trace-test-only native event trigger (fires a workspace signal with
    // no payload, producing a commandless OBSERVE capture on the debounce).
    fire: (name: string) => void;
    lastCall: (method: string) => { method: string; payload: string; callback: (reply: unknown) => void; answered: boolean };
}

function makeHarness(extraDesktops: readonly string[] = []): Harness {
    const wins = [
        makeWin(WIN_A, { x: TILE_A[0], y: TILE_A[1], width: TILE_A[2], height: TILE_A[3] }),
        makeWin(WIN_B, { x: TILE_B[0], y: TILE_B[1], width: TILE_B[2], height: TILE_B[3] }),
    ];
    const world = {
        screens: [{ name: OUT, geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [{ id: WS }, ...extraDesktops.map((id) => ({ id }))],
        wins,
        activeScreen: { name: OUT },
        activeWindow: wins[0] as FakeWin | null,
        currentDesktop: { id: WS },
    };
    const calls: Harness["calls"] = [];
    const timers: Harness["timers"] = [];
    const logs: string[] = [];
    const signals: Record<string, ReturnType<typeof fakeSignal>> = {
        windowAdded: fakeSignal(), windowRemoved: fakeSignal(), windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(), screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(), activeScreenChanged: fakeSignal(),
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens, desktops: world.desktops, activeScreen: world.activeScreen,
        get activeWindow() { return world.activeWindow; },
        set activeWindow(_ref: unknown) {
            const found = world.wins.find((w) => (w as unknown) === _ref);
            if (found !== undefined) {
                world.activeWindow = found;
            }
        },
        get currentDesktop() { return world.currentDesktop; },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        windowAdded: signals["windowAdded"]!.signal, windowRemoved: signals["windowRemoved"]!.signal,
        windowActivated: signals["windowActivated"]!.signal, desktopsChanged: signals["desktopsChanged"]!.signal,
        screensChanged: signals["screensChanged"]!.signal,
        currentDesktopChanged: signals["currentDesktopChanged"]!.signal,
        activeScreenChanged: signals["activeScreenChanged"]!.signal,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface, owner: "owner-a", adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            const call = { method, payload, callback, answered: false };
            calls.push(call);
            if (method === "QueueWorldCommand") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                call.answered = true;
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: body["gen"], client: body["client"], clientCmd: body["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const clientCmd = body["clientCmd"] as string;
                const takeMap = (globalThis as Record<string, unknown>)["__traceTakeMap"] as Record<string, unknown> | undefined;
                if (takeMap !== undefined && Object.prototype.hasOwnProperty.call(takeMap, clientCmd)) {
                    const staged = takeMap[clientCmd] as Record<string, unknown>;
                    delete takeMap[clientCmd];
                    call.answered = true;
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: body["gen"], globalCmd: "1", result: staged }));
                    return;
                }
                const staged = (globalThis as Record<string, unknown>)["__traceTake"] as Record<string, unknown> | undefined;
                const stagedFor = (globalThis as Record<string, unknown>)["__traceTakeFor"] as string | undefined;
                call.answered = true;
                if (staged !== undefined && (stagedFor === undefined || stagedFor === body["clientCmd"])) {
                    delete (globalThis as Record<string, unknown>)["__traceTake"];
                    delete (globalThis as Record<string, unknown>)["__traceTakeFor"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: body["gen"], globalCmd: "1", result: staged }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: body["gen"] }));
                }
            } else if (method === "ReadNativePreimage") {
                const corr = (JSON.parse(payload) as Record<string, unknown>)["correlation_id"] as string;
                call.answered = true;
                callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "ok", records: [], stored: 0, returned: 0 }));
            }
        },
        scheduleOnce: (delay, cb) => { timers.push({ delay, cb }); return () => undefined; },
        log: (m) => { logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) { timers.splice(i, 1); i -= 1; t.cb(); }
        }
    };
    return {
        wins, activeId: WIN_A, calls, timers, logs, handle, runTimers,
        setActive: (id: string | null) => {
            world.activeWindow = id === null ? null : (world.wins.find((w) => w.internalId === id) ?? null);
        },
        fire: (name: string) => {
            const sig = signals[name];
            assert.ok(sig !== undefined, `known signal ${name}`);
            for (const handler of [...sig.handlers]) {
                (handler as (...a: Array<unknown>) => void)();
            }
        },
        lastCall: (method: string) => {
            const f = [...calls].reverse().find((c) => c.method === method);
            assert.ok(f !== undefined, `expected ${method}`);
            return f;
        },
    };
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    const { take, takeFor, takeMap, ...snapshot } = extra;
    if (takeMap !== undefined) {
        (globalThis as Record<string, unknown>)["__traceTakeMap"] = { ...(takeMap as Record<string, unknown>) };
    }
    if (take !== undefined) {
        (globalThis as Record<string, unknown>)["__traceTake"] = take;
        if (takeFor !== undefined) {
            (globalThis as Record<string, unknown>)["__traceTakeFor"] = takeFor;
        } else {
            delete (globalThis as Record<string, unknown>)["__traceTakeFor"];
        }
    }
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false, source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null, intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: OUT, ws: WS }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
        ...snapshot,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function discover(h: Harness, gen = "gen-1"): void {
    const handshake = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try { return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake"; } catch { return false; }
    });
    assert.ok(handshake !== undefined, "expected handshake");
    handshake.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter",
        req: (JSON.parse(handshake.payload) as Record<string, unknown>)["req"], ready: false, kind: "", gen,
    }));
    const settings = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try { return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings"; } catch { return false; }
    });
    assert.ok(settings !== undefined, "expected settings");
    const settingsBody = JSON.parse(settings.payload) as Record<string, unknown>;
    settings.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: settingsBody["req"], rev: settingsBody["rev"], status: "accepted", diagnostics: [],
    }));
    h.runTimers();
    const auto = h.lastCall("DescribeWorld");
    const body = JSON.parse(auto.payload) as Record<string, unknown>;
    auto.callback(acceptedReply(gen, body["source"] as string, body["nativeInvalid"] as string, "0"));
}

function winRow(win: string, over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
        win, out: OUT, ws: WS, alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "",
        frame_op: "", floating: false, float_origin: "", clear_op: "", cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
        ...over,
    };
}

function st(property: string, value: boolean, auth = "controlled", reason = "", cmdSeq: string | null = null): Record<string, unknown> {
    return { property, value, auth, reason, cmdSeq };
}

function pressFloat(h: Harness): Record<string, unknown> {
    h.handle.requestFloat();
    h.runTimers();
    return JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
}

function answerDescribe(h: Harness, req: Record<string, unknown>, rev: string, extra: Record<string, unknown> = {}): void {
    h.lastCall("DescribeWorld").callback(
        acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev, extra),
    );
}

function answerOldestWrite(h: Harness): void {
    const call = h.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered);
    assert.ok(call !== undefined, "expected pending WriteNativePreimage");
    call.answered = true;
    const corr = (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
    const n = ((JSON.parse(call.payload) as Record<string, unknown>)["records"] as unknown[]).length;
    call.callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "stored", stored: n }));
}

function ackWriteIfPending(h: Harness): void {
    const call = h.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered);
    if (call === undefined) {
        return;
    }
    call.answered = true;
    const corr = (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
    const n = ((JSON.parse(call.payload) as Record<string, unknown>)["records"] as unknown[]).length;
    call.callback(JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "stored", stored: n }));
}

const traceLines = (h: Harness, ev: string): string[] =>
    h.logs.filter((l) => l.includes("omnitiler:world:trace") && l.includes(`ev=${ev}`));

describe("world trace diagnostics (rapid float and sticky)", () => {
    it("rapid float spam records per-press subjects and per-apply plans with attempts", () => {
        const h = makeHarness();
        discover(h);
        // Six-press spam alternating float on/off on the focused subject.
        // Float-on replies stay untouched (session placement, no core
        // geometry write, member untouched so the float registers);
        // unfloat replies carry controlled tile frame+member plans so the
        // float clears and the converging writes record frame attempts.
        // Each cycle drains catchup captures and persist acks first:
        // park-first FIFO queues each press behind any debounce or flight
        // instead of refusing as busy, so each press sends exactly its own
        // flight in press order.
        let floating = false;
        let answeredDesc = 1; // discover() settles the auto capture.
        let rev = 0;
        const nextRev = (): string => String(++rev);
        const descCalls = (): Array<{ payload: string; callback: (reply: unknown) => void }> =>
            h.calls.filter((c) => c.method === "DescribeWorld");
        for (let press = 1; press <= 6; press += 1) {
            floating = !floating;
            assert.equal(descCalls().length, answeredDesc, `world idle before press ${press}`);
            const before = descCalls().length;
            h.handle.requestFloat();
            h.runTimers();
            assert.equal(descCalls().length, before + 1, `press ${press} sends exactly its flight`);
            const req = JSON.parse(descCalls()[descCalls().length - 1]!.payload) as Record<string, unknown>;
            const frame = floating ? FLOAT_A : TILE_A;
            descCalls()[descCalls().length - 1]!.callback(
                acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, nextRev(), {
                    windows: [
                        winRow(WIN_A, {
                            floating,
                            frame_auth: floating ? "untouched" : "controlled", frame_hold: "",
                            member_auth: floating ? "untouched" : "controlled", member_hold: "",
                            frame, alloc: frame,
                        }),
                        winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: TILE_B, alloc: TILE_B }),
                    ],
                    ...(press === 2 ? { take: { op: "toggle-float", win: WIN_A, rect: [...FLOAT_A] }, takeFor: "2" } : {}),
                }),
            );
            answeredDesc += 1;
            ackWriteIfPending(h);
            // Drain commandless catchups neutrally (mirrored live, untouched
            // authority: no writes, no membership edits, no float edits).
            for (let round = 0; round < 4; round += 1) {
                h.runTimers();
                ackWriteIfPending(h);
                if (descCalls().length === answeredDesc) {
                    break;
                }
                assert.equal(descCalls().length, answeredDesc + 1, `at most one flight outstanding (press ${press})`);
                const catchup = descCalls()[descCalls().length - 1]!;
                const body = JSON.parse(catchup.payload) as Record<string, unknown>;
                const liveA = h.wins[0] as unknown as { frameGeometry: { x: number; y: number; width: number; height: number } };
                const liveB = h.wins[1] as unknown as { frameGeometry: { x: number; y: number; width: number; height: number } };
                catchup.callback(
                    acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, nextRev(), {
                        windows: [
                            winRow(WIN_A, {
                                floating, frame_auth: "untouched", frame_hold: "",
                                member_auth: "untouched", member_hold: "",
                                frame: [liveA.frameGeometry.x, liveA.frameGeometry.y, liveA.frameGeometry.width, liveA.frameGeometry.height],
                                alloc: [liveA.frameGeometry.x, liveA.frameGeometry.y, liveA.frameGeometry.width, liveA.frameGeometry.height],
                            }),
                            winRow(WIN_B, {
                                floating: false, frame_auth: "untouched", frame_hold: "",
                                member_auth: "untouched", member_hold: "",
                                frame: [liveB.frameGeometry.x, liveB.frameGeometry.y, liveB.frameGeometry.width, liveB.frameGeometry.height],
                                alloc: [liveB.frameGeometry.x, liveB.frameGeometry.y, liveB.frameGeometry.width, liveB.frameGeometry.height],
                            }),
                        ],
                    }),
                );
                answeredDesc += 1;
                ackWriteIfPending(h);
            }
            assert.equal(descCalls().length, answeredDesc, `world idle after press ${press}`);
            if (floating) {
                // Session placement positions the newly floated window (the
                // untouched float reply writes no geometry): the next unfloat
                // then carries this live frame and its controlled tile plan
                // records a converging write.
                const live = h.wins[0] as unknown as {
                    frameGeometry: { x: number; y: number; width: number; height: number };
                    clientGeometry: { x: number; y: number; width: number; height: number };
                };
                live.frameGeometry = { x: FLOAT_A[0], y: FLOAT_A[1], width: FLOAT_A[2], height: FLOAT_A[3] };
                live.clientGeometry = { x: FLOAT_A[0], y: FLOAT_A[1], width: FLOAT_A[2], height: FLOAT_A[3] };
            }
        }
        const cmds = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-float"));
        assert.equal(cmds.length, 6, `six float presses leave six ev=cmd lines: ${cmds.join("\n")}`);
        for (let i = 0; i < 6; i += 1) {
            const line = cmds[i] as string;
            assert.ok(line.includes(`clientCmd=${i + 1}`), `press ${i + 1} carries its clientCmd: ${line}`);
            assert.ok(line.includes(`press=${WIN_A}`), `press ${i + 1} retains its press basis: ${line}`);
            assert.ok(line.includes("win=currentFocus"), `press ${i + 1} carries the submitted marker: ${line}`);
            assert.ok(line.includes(`focusPress=${WIN_A}`), `press ${i + 1} records adapter focus: ${line}`);
        }
        // Float-on carries no rect (session placement wins); unfloat carries
        // the live float frame positioned after the float-on.
        assert.ok(cmds[0]!.includes("rect=-"), `float-on carries no rect: ${cmds[0]}`);
        assert.ok(cmds[1]!.includes(`rect=${FLOAT_A.join(",")}`), `unfloat carries the live float frame: ${cmds[1]}`);
        const subjects = traceLines(h, "subject").filter((l) => l.includes(`win=${WIN_A}`));
        assert.equal(subjects.length, 6, `six applies leave six ev=subject lines: ${subjects.join("\n")}`);
        for (const line of subjects) {
            assert.ok(line.includes("core="), `subject line carries the core plan: ${line}`);
            assert.ok(line.includes("fact="), `subject line carries live native facts: ${line}`);
            assert.ok(line.includes("basis=take") || line.includes("basis=press"),
                `press-window row is honestly labeled take or press, never guessed: ${line}`);
        }
        // The staged take result resolves the marker subject and rides the
        // exact take line; unresolved presses ride press-basis rows.
        const takes = subjects.filter((l) => l.includes("take=toggle-float@"));
        assert.ok(takes.length >= 1, `a staged take result is attributed: ${subjects.join("\n")}`);
        assert.ok(takes.some((l) => l.includes("basis=take")), `staged take rides a take-basis line: ${takes.join("\n")}`);
        // Ordinary converged writes record accepted frame attempts with the
        // post-write readback matching the plan.
        const frames = traceLines(h, "frame-attempt").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(frames.length >= 1, `converged writes leave frame attempts: ${h.logs.filter((l) => l.includes("frame-attempt")).join("\n")}`);
        for (const line of frames) {
            assert.ok(line.includes("setter=accepted"), `attempt carries the raw setter: ${line}`);
            assert.ok(line.includes("applySrc="), `converged attempt names its proven apply boundary: ${line}`);
        }
        h.handle.stop();
    });

    it("take rendering carries independent noSpace and rejected kind", () => {
        // No-space refusal terminal: toggle-float carries rect null plus
        // noSpace true on the same take, both rendered independently.
        const h = makeHarness();
        discover(h);
        h.handle.requestFloat();
        h.runTimers();
        const queued = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = queued["clientCmd"] as string;
        const req = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req, "1", {
            windows: [
                winRow(WIN_A, { floating: true, frame_auth: "untouched", frame_hold: "", frame: [...FLOAT_A], alloc: [...FLOAT_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
            takeMap: { [cmd]: { op: "toggle-float", win: WIN_A, rect: null, noSpace: true } },
        });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        const subjects = traceLines(h, "subject").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(subjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes("basis=take") && l.includes("take=toggle-float@-@noSpace=1")),
            `refusal take keeps rect and noSpace on its take-basis line: ${subjects.join("\n")}`);
        h.handle.stop();
        // Rejected kind terminal: the kind rides the same take display
        // under the exact requesting clientCmd.
        const h2 = makeHarness();
        discover(h2);
        h2.handle.requestFloat();
        h2.runTimers();
        const queued2 = JSON.parse(h2.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd2 = queued2["clientCmd"] as string;
        const req2 = JSON.parse(h2.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h2, req2, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
            takeMap: { [cmd2]: { op: "rejected", kind: "focus-unknown" } },
        });
        h2.runTimers();
        ackWriteIfPending(h2);
        h2.runTimers();
        const subjects2 = traceLines(h2, "subject").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(subjects2.some((l) => l.includes(`clientCmd=${cmd2}`) && l.includes("basis=press") && l.includes("take=rejected@focus-unknown")),
            `rejected take keeps its kind on the press-basis line: ${subjects2.join("\n")}`);
        h2.handle.stop();
    });

    it("a stuck subject is located by planned-vs-live frames while the sibling converges", () => {
        const h = makeHarness();
        discover(h);
        // Freeze the subject's native frame: the setter silently refuses
        // while the sibling keeps converging.
        const stuck = h.wins.find((w) => w.internalId === WIN_A) as unknown as Record<string, unknown>;
        const frozen = { x: 0, y: 0, width: 200, height: 200 };
        Object.defineProperty(stuck, "frameGeometry", {
            configurable: true,
            get: () => ({ ...frozen }),
            set: () => undefined,
        });
        const req = pressFloat(h);
        answerDescribe(h, req, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "controlled", frame_hold: "", frame: [0, 0, 400, 600], alloc: [0, 0, 400, 600] }),
                winRow(WIN_B, { floating: false, frame_auth: "controlled", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        const subjects = traceLines(h, "subject");
        const subjectA = subjects.find((l) => l.includes(`win=${WIN_A}`));
        const subjectB = subjects.find((l) => l.includes(`win=${WIN_B}`));
        assert.ok(subjectA !== undefined, `subject line for the stuck window: ${subjects.join("\n")}`);
        assert.ok(subjectB !== undefined, `subject line for the sibling: ${subjects.join("\n")}`);
        // The plan names the core frame; the fact names the live frame. The
        // stuck subject shows a planned/live mismatch on one line while the
        // sibling shows convergence.
        assert.ok(subjectA.includes("core=0/0,0,400,600/0,0,400,600"), `stuck plan is explicit: ${subjectA}`);
        assert.ok(subjectA.includes("fact=0,0,200,200"), `stuck live frame is explicit: ${subjectA}`);
        const coreFrame = subjectA.match(/core=[01]\/([0-9,]+)\//);
        const factFrame = subjectA.match(/fact=([0-9,]+)/);
        assert.ok(coreFrame !== null && factFrame !== null, `frames parse: ${subjectA}`);
        assert.notEqual(coreFrame[1], factFrame[1], `plan and live differ on the stuck line: ${subjectA}`);
        assert.ok(subjectB.includes("fact=400,0,400,600"), `sibling live frame converges: ${subjectB}`);
        // Aggregate counts alone cannot name the stuck window: geometry moved
        // only for the sibling while the subject held.
        const applied = h.logs.filter((l) => l.includes("omnitiler:world:applied rev=1 "));
        assert.equal(applied.length, 1, `one applied line: ${applied.join("\n")}`);
        assert.ok(applied[0]!.includes("geom=1"), `aggregate counts one geometry write: ${applied[0]}`);
        assert.ok(!applied[0]!.includes(WIN_A) && !applied[0]!.includes(WIN_B),
            `aggregate counts name no window: ${applied[0]}`);
        h.handle.stop();
    });

    it("tile sticky on/off records state targets and native attempts per subject", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestSticky();
        h.runTimers();
        const req = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req, "1", {
            windows: [winRow(WIN_A, { state: [st("sticky", true, "controlled", "", "1"), st("above", true, "controlled", "", "1")] })],
        });
        answerOldestWrite(h);
        h.runTimers();
        const cmds = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-sticky"));
        assert.equal(cmds.length, 1, `sticky-on leaves one ev=cmd: ${cmds.join("\n")}`);
        assert.ok(cmds[0]!.includes(`press=${WIN_A}`), `sticky retains its press basis: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes("win=currentFocus"), `sticky carries the submitted marker: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes(`focusPress=${WIN_A}`), `sticky records adapter focus: ${cmds[0]}`);
        const subjects = traceLines(h, "subject").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(subjects.length >= 1, `sticky apply leaves a subject line: ${subjects.join("\n")}`);
        assert.ok(subjects.some((l) => l.includes("basis=press") && l.includes("sticky=1/controlled") && l.includes("above=1/controlled")),
            `press-basis line carries both state targets: ${subjects.join("\n")}`);
        // Native executor attempts record on the persist ack after the
        // applied diagnostic, so they flush on the next send: start the
        // release press (which sends) before asserting the on-attempts.
        h.handle.requestSticky();
        h.runTimers();
        const attempts = traceLines(h, "state-attempt").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(attempts.some((l) => l.includes("property=sticky") && l.includes("target=1") && l.includes("setter=accepted")),
            `sticky attempt is recorded: ${attempts.join("\n")}`);
        assert.ok(attempts.some((l) => l.includes("property=above") && l.includes("target=1") && l.includes("setter=accepted")),
            `above attempt is recorded: ${attempts.join("\n")}`);
        // Sticky release from the same tile baseline.
        const req2 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req2, "2", {
            windows: [winRow(WIN_A, { state: [st("sticky", false, "controlled", "", "2"), st("above", false, "controlled", "", "2")] })],
        });
        answerOldestWrite(h);
        h.runTimers();
        const offs = traceLines(h, "subject").filter((l) => l.includes(`win=${WIN_A}`) && l.includes("sticky=0/controlled"));
        assert.ok(offs.length >= 1, `release apply carries cleared targets: ${traceLines(h, "subject").join("\n")}`);
        h.handle.stop();
    });

    it("float sticky on/off/unfloat roundtrip keeps per-press attribution", () => {
        const h = makeHarness();
        discover(h);
        // Float on.
        const req1 = pressFloat(h);
        answerDescribe(h, req1, "1", {
            windows: [
                winRow(WIN_A, { floating: true, frame_auth: "untouched", frame_hold: "", frame: [...FLOAT_A], alloc: [...FLOAT_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        // Sticky on from float.
        h.handle.requestSticky();
        h.runTimers();
        const req2 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req2, "2", {
            windows: [
                winRow(WIN_A, { floating: true, frame_auth: "untouched", frame_hold: "", frame: [...FLOAT_A], alloc: [...FLOAT_A], state: [st("sticky", true, "controlled", "", "2"), st("above", true, "controlled", "", "2")] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        answerOldestWrite(h);
        h.runTimers();
        // Sticky off back to prior float.
        h.handle.requestSticky();
        h.runTimers();
        const req3 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req3, "3", {
            windows: [
                winRow(WIN_A, { floating: true, frame_auth: "untouched", frame_hold: "", frame: [...FLOAT_A], alloc: [...FLOAT_A], state: [st("sticky", false, "controlled", "", "3"), st("above", false, "controlled", "", "3")] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        answerOldestWrite(h);
        h.runTimers();
        // Unfloat back to tile.
        const req4 = pressFloat(h);
        answerDescribe(h, req4, "4", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        const cmds = traceLines(h, "cmd");
        const ops = cmds.map((l) => (l.match(/op=([a-z-]+)/) as RegExpMatchArray)[1]);
        assert.deepEqual(ops, ["toggle-float", "toggle-sticky", "toggle-sticky", "toggle-float"],
            `roundtrip keeps press order with subjects: ${cmds.join("\n")}`);
        assert.ok(cmds.every((l) => l.includes(`press=${WIN_A}`) && l.includes("win=currentFocus") && l.includes(`focusPress=${WIN_A}`)),
            `every press retains press basis, submitted marker and focus: ${cmds.join("\n")}`);
        const subjects = traceLines(h, "subject").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(subjects.length >= 4, `every apply attributes the subject: ${subjects.join("\n")}`);
        assert.ok(subjects.every((l) => l.includes("basis=press") || l.includes("basis=take") || l.includes("basis=submitted") || l.includes("basis=affected")),
            `every row carries an honest basis: ${subjects.join("\n")}`);
        const last = subjects[subjects.length - 1] as string;
        assert.ok(last.includes("core=0/"), `final apply returns to tile: ${last}`);
        h.handle.stop();
    });

    it("identifier contract preserves dotted underscored long window ids", () => {
        const h = makeHarness();
        discover(h);
        const LONG = `win.dotted_under-${"x".repeat(83)}`;
        assert.equal(LONG.length, 100, "long id is ~100 chars");
        assert.ok(LONG.includes(".") && LONG.includes("_"), "long id covers both separators");
        h.wins[0]!.internalId = LONG;
        h.setActive(LONG);
        const req = pressFloat(h);
        answerDescribe(h, req, "1", {
            windows: [
                winRow(LONG, { floating: true, frame_auth: "untouched", frame_hold: "", frame: [...FLOAT_A], alloc: [...FLOAT_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        const cmds = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-float"));
        assert.equal(cmds.length, 1, `one float press leaves one ev=cmd: ${cmds.join("\n")}`);
        assert.ok(cmds[0]!.includes(`press=${LONG}`), `ev=cmd retains the full dotted press id: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes("win=currentFocus"), `ev=cmd carries the submitted marker: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes(`focusPress=${LONG}`), `ev=cmd carries the full focus id: ${cmds[0]}`);
        assert.ok(!cmds[0]!.includes("win=-"), `long id is not collapsed: ${cmds[0]}`);
        const subjects = traceLines(h, "subject").filter((l) => l.includes(`win=${LONG}`));
        assert.ok(subjects.length >= 1, `apply carries the full press-basis id: ${h.logs.filter((l) => l.includes("ev=subject")).join("\n")}`);
        assert.ok(subjects.some((l) => l.includes("basis=press")), `long-id row is honestly press-basis: ${subjects.join("\n")}`);
        h.handle.stop();
    });

    it("late persist acks flush exactly once under the delivering source", () => {
        const h = makeHarness();
        discover(h);
        // Sticky-on flight S1: answer but hold the persist ack so the
        // post-apply executor attempts land after the applied diagnostic.
        h.handle.requestSticky();
        h.runTimers();
        const req1 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        const src1 = req1["source"] as string;
        assert.ok(typeof src1 === "string" && src1.length > 0, "S1 has a source");
        answerDescribe(h, req1, "1", {
            windows: [winRow(WIN_A, { state: [st("sticky", true, "controlled", "", "1"), st("above", true, "controlled", "", "1")] })],
        });
        h.runTimers();
        answerOldestWrite(h);
        h.runTimers();
        // Second press (sticky-off) sends S2: the S1 on-attempts flush here.
        h.handle.requestSticky();
        h.runTimers();
        const req2 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        const src2 = req2["source"] as string;
        assert.ok(typeof src2 === "string" && src2.length > 0 && src2 !== src1, "S2 is a new source");
        const onAttempts = traceLines(h, "state-attempt").filter((l) => l.includes(`win=${WIN_A}`));
        assert.ok(onAttempts.length >= 2, `on-attempts flushed at S2 send: ${onAttempts.join("\n")}`);
        for (const line of onAttempts) {
            assert.ok(line.includes(`flushSrc=${src2}`), `on-attempt names the delivering S2 source: ${line}`);
            assert.ok(!line.includes("applySrc="), `late ack is never labeled a proven apply: ${line}`);
            assert.ok(!/(?:^| )src=/.test(line), `attempt carries no bare inferred src: ${line}`);
        }
        const onSticky = onAttempts.filter((l) => l.includes("property=sticky") && l.includes("target=1"));
        const onAbove = onAttempts.filter((l) => l.includes("property=above") && l.includes("target=1"));
        assert.equal(onSticky.length, 1, `sticky-on emits once: ${onAttempts.join("\n")}`);
        assert.equal(onAbove.length, 1, `above-on emits once: ${onAttempts.join("\n")}`);
        // Answer S2 with the cleared state, ack its persist, then a third
        // press sends S3 and flushes the off-attempts exactly once.
        answerDescribe(h, req2, "2", {
            windows: [winRow(WIN_A, { state: [st("sticky", false, "controlled", "", "2"), st("above", false, "controlled", "", "2")] })],
        });
        h.runTimers();
        answerOldestWrite(h);
        h.runTimers();
        h.handle.requestSticky();
        h.runTimers();
        const req3 = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        const src3 = req3["source"] as string;
        assert.ok(typeof src3 === "string" && src3.length > 0 && src3 !== src2, "S3 is a new source");
        const all = traceLines(h, "state-attempt").filter((l) => l.includes(`win=${WIN_A}`));
        const offAttempts = all.filter((l) => l.includes("target=0"));
        assert.ok(offAttempts.length >= 2, `off-attempts flushed at S3 send: ${offAttempts.join("\n")}`);
        for (const line of offAttempts) {
            assert.ok(line.includes(`flushSrc=${src3}`), `off-attempt names the delivering S3 source: ${line}`);
            assert.ok(!line.includes("applySrc="), `late ack is never labeled a proven apply: ${line}`);
        }
        const offSticky = all.filter((l) => l.includes("property=sticky") && l.includes("target=0"));
        const offAbove = all.filter((l) => l.includes("property=above") && l.includes("target=0"));
        assert.equal(offSticky.length, 1, `sticky-off emits once: ${all.join("\n")}`);
        assert.equal(offAbove.length, 1, `above-off emits once: ${all.join("\n")}`);
        // The on-rows are never re-emitted under a later source.
        assert.equal(all.filter((l) => l.includes("property=sticky") && l.includes("target=1")).length, 1,
            `sticky-on still exactly once: ${all.join("\n")}`);
        assert.equal(all.filter((l) => l.includes("property=above") && l.includes("target=1")).length, 1,
            `above-on still exactly once: ${all.join("\n")}`);
        h.handle.stop();
    });

    it("implicit focus subjects resolve only by exact clientCmd take", () => {
        // Leg 1: no takes staged - both implicit presses stay unassigned.
        // Park-first FIFO sends one flight per press: answer each flight
        // in turn so both applies run before asserting.
        const h1 = makeHarness();
        discover(h1);
        h1.setActive(null);
        h1.handle.requestFocus("left");
        h1.runTimers();
        h1.handle.requestFocus("left");
        h1.runTimers();
        const winRows12 = [
            winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
            winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
        ];
        {
            const descs = h1.calls.filter((c) => c.method === "DescribeWorld");
            const reqA = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
            answerDescribe(h1, reqA, "1", { windows: winRows12 });
            h1.runTimers();
            ackWriteIfPending(h1);
            h1.runTimers();
            const descs2 = h1.calls.filter((c) => c.method === "DescribeWorld");
            if (descs2.length > descs.length) {
                const reqB = JSON.parse(descs2[descs2.length - 1]!.payload) as Record<string, unknown>;
                answerDescribe(h1, reqB, "2", { windows: winRows12 });
                h1.runTimers();
                ackWriteIfPending(h1);
                h1.runTimers();
            }
        }
        const cmds1 = traceLines(h1, "cmd").filter((l) => l.includes("op=focus-directional"));
        assert.equal(cmds1.length, 2, `two nofocus presses leave two ev=cmd: ${cmds1.join("\n")}`);
        const subjects1 = traceLines(h1, "subject");
        const un1 = traceLines(h1, "marker-unassigned");
        assert.equal(subjects1.length, 0, `no take means no guessed subjects: ${subjects1.join("\n")}`);
        assert.equal(un1.length, 2, `both presses stay unassigned: ${un1.join("\n")}`);
        assert.ok(un1.every((l) => l.includes("result=pending")), `unassigned evidence is pending: ${un1.join("\n")}`);
        h1.handle.stop();
        // Leg 2: only clientCmd 2 has a broker-attributed win - cmd1 must
        // stay unassigned, never inherit cmd2's win by order. The take map
        // is staged inter-flight (keyed by clientCmd, after the first
        // flight's apply so the cmd1 flight cannot consume cmd2's entry);
        // both sequential flights are then drained.
        const h2 = makeHarness();
        discover(h2);
        h2.setActive(null);
        h2.handle.requestFocus("left");
        h2.runTimers();
        h2.handle.requestFocus("left");
        h2.runTimers();
        {
            const descs = h2.calls.filter((c) => c.method === "DescribeWorld");
            const reqA = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
            answerDescribe(h2, reqA, "1", { windows: winRows12 });
            h2.runTimers();
            ackWriteIfPending(h2);
            h2.runTimers();
            // Stage cmd2's broker attribution only now: the first flight's
            // undecided-wide take collection must leave cmd2 pending (else
            // the ready result is consumed by a flight whose batch excludes
            // cmd2 and never reaches any apply).
            (globalThis as Record<string, unknown>)["__traceTakeMap"] = {
                "2": { op: "directional", outcome: "focus-unfocused-retained", win: WIN_B, rect: null, cause: null },
            };
            const descs2 = h2.calls.filter((c) => c.method === "DescribeWorld");
            if (descs2.length > descs.length) {
                const reqB = JSON.parse(descs2[descs2.length - 1]!.payload) as Record<string, unknown>;
                answerDescribe(h2, reqB, "2", { windows: winRows12 });
                h2.runTimers();
                ackWriteIfPending(h2);
                h2.runTimers();
            }
        }
        const subjects2 = traceLines(h2, "subject");
        const un2 = traceLines(h2, "marker-unassigned");
        assert.ok(subjects2.some((l) => l.includes("clientCmd=2") && l.includes(`win=${WIN_B}`)),
            `cmd2 resolves to its own win: ${subjects2.join("\n")}`);
        assert.equal(subjects2.filter((l) => l.includes("clientCmd=2")).length, 1,
            `cmd2 resolves exactly once: ${subjects2.join("\n")}`);
        assert.equal(un2.length, 1, `cmd1 stays unassigned: ${un2.join("\n")}`);
        assert.ok(un2[0]!.includes("clientCmd=1"), `unassigned is cmd1: ${un2[0]}`);
        assert.ok(un2[0]!.includes("result=pending"), `cmd1 evidence is pending: ${un2[0]}`);
        assert.ok(!subjects2.some((l) => l.includes("clientCmd=1")), `no guessed subject for cmd1: ${subjects2.join("\n")}`);
        h2.handle.stop();
    });
});

describe("world trace subject-basis display caps", () => {
    // Sticky uses the ordinary submitCommand park-first FIFO path (the same
    // toImplicitSubject marker conversion as float); float itself cannot
    // batch same-tick because native.submitFloat fail-fast fences while a
    // command is parked or a flight is outstanding.
    //
    // Batching pattern: the first press sends its own flight; presses parked
    // while that flight is outstanding drain in ONE follow-up capture.
    // Takes are staged only on the batched flight's reply: the earlier
    // accept collects pending (retaining undecided identities), so the
    // batched accept resolves every take exactly once.
    it("batched eight-command flight emits at most eight combined subject rows", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestSticky();
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "QueueWorldCommand").length, 1, `first press files one queue command`);
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length - 1, 1, `first press sends its own flight`);
        for (let i = 0; i < 8; i += 1) {
            h.handle.requestSticky();
        }
        h.runTimers();
        const queues = h.calls.filter((c) => c.method === "QueueWorldCommand");
        assert.equal(queues.length, 9, `all nine presses filed while the first flight was outstanding`);
        // Answer the first flight commandless-neutral; its take collection
        // stays pending so every identity survives for the batched flight.
        const firstReq = JSON.parse(h.calls.filter((c) => c.method === "DescribeWorld")[1]!.payload) as Record<string, unknown>;
        answerDescribe(h, firstReq, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 3, `one follow-up capture drains the eight parked commands (auto + first + batch)`);
        const batchCmds = queues.slice(queues.length - 8).map((c) => (JSON.parse(c.payload) as Record<string, unknown>)["clientCmd"] as string);
        for (const q of queues.slice(queues.length - 8)) {
            const wire = (JSON.parse(q.payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
            assert.deepEqual(wire["win"], { currentFocus: true }, `batched press queues the marker: ${JSON.stringify(wire)}`);
        }
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const batchSrc = req["source"] as string;
        const takeMap: Record<string, unknown> = {};
        const windows: Record<string, unknown>[] = [];
        for (let i = 0; i < 8; i += 1) {
            const target = `win-${i + 1}`;
            // Synthetic take attribution: only the win string and exact
            // clientCmd key matter to the display logic under test.
            takeMap[batchCmds[i] as string] = { op: "toggle-float", win: target, rect: null };
            windows.push(winRow(target, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }));
        }
        windows.push(winRow("win-9", { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }));
        answerDescribe(h, req, "2", { windows, takeMap });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        const batchCmdsLogged = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-sticky"));
        assert.equal(batchCmdsLogged.length, 9, `all nine presses leave ev=cmd lines: ${batchCmdsLogged.join("\n")}`);
        for (const cmd of batchCmds) {
            assert.ok(batchCmdsLogged.some((l) => l.includes(`clientCmd=${cmd}`)), `batched cmd ${cmd} logged: ${batchCmdsLogged.join("\n")}`);
        }
        const batchSubjects = traceLines(h, "subject").filter((l) => l.includes(`src=${batchSrc}`));
        for (const cmd of batchCmds) {
            assert.ok(batchSubjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes("basis=take")),
                `batched cmd ${cmd} resolves its own take-basis row in the shared flight: ${batchSubjects.join("\n")}`);
        }
        assert.ok(batchSubjects.length <= 8, `combined command/press/affected rows stay capped at eight: ${batchSubjects.join("\n")}`);
        h.handle.stop();
    });

    it("batched same-win two-command flight resolves one row per command", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestSticky();
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length - 1, 1, `first press sends its own flight`);
        // Production queue path: park both presses while the first flight
        // is outstanding so one follow-up capture carries both commands.
        h.handle.requestSticky();
        h.handle.requestSticky();
        h.runTimers();
        const queues = h.calls.filter((c) => c.method === "QueueWorldCommand");
        assert.equal(queues.length, 3, `all three presses filed while the first flight was outstanding`);
        const firstReq = JSON.parse(h.calls.filter((c) => c.method === "DescribeWorld")[1]!.payload) as Record<string, unknown>;
        answerDescribe(h, firstReq, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
        });
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 3, `one follow-up capture carries the two-command batch (auto + first + batch)`);
        const batchCmds = queues.slice(queues.length - 2).map((c) => (JSON.parse(c.payload) as Record<string, unknown>)["clientCmd"] as string);
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const batchSrc = req["source"] as string;
        answerDescribe(h, req, "2", {
            windows: [
                winRow(WIN_A, { state: [st("sticky", true, "controlled", "", "2"), st("above", true, "controlled", "", "2")] }),
            ],
            takeMap: {
                // Synthetic take attribution (same valid float-take shape
                // the existing conversion test stages): only the exact
                // clientCmd key and win string matter to the display logic.
                [batchCmds[0] as string]: { op: "toggle-float", win: WIN_A, rect: null },
                [batchCmds[1] as string]: { op: "toggle-float", win: WIN_A, rect: null },
            },
        });
        answerOldestWrite(h);
        h.runTimers();
        const batchSubjects = traceLines(h, "subject").filter((l) => l.includes(`src=${batchSrc}`));
        for (const cmd of batchCmds) {
            assert.ok(batchSubjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes(`win=${WIN_A}`) && l.includes("basis=take")),
                `cmd ${cmd} resolves its own take-basis row to the shared win: ${batchSubjects.join("\n")}`);
        }
        const batchUn = traceLines(h, "marker-unassigned").filter((l) => l.includes(`src=${batchSrc}`));
        assert.equal(batchUn.length, 0, `resolved commands never spill to marker-unassigned: ${batchUn.join("\n")}`);
        h.handle.stop();
    });
});

describe("world trace observe-flight reconnect diagnostics", () => {
    const observeLines = (h: Harness): string[] =>
        h.logs.filter((l) => l.includes("omnitiler:world:trace") &&
            /ev=(observe-sent|observe-sent-win|observe-retry|observe-applied|observe-fresh-win|observe-plan|observe-bind|observe-transfer|member-attempt)/.test(l));
    const srcOf = (line: string): string => {
        const m = line.match(/ src=([a-z0-9-]+)/);
        assert.ok(m !== null, `observe line carries a source: ${line}`);
        return (m as RegExpMatchArray)[1] as string;
    };

    it("commandless OBSERVE flights emit shared-source sent-vs-fresh receipt, plan, bind and transfer evidence", () => {
        const h = makeHarness();
        discover(h);
        // A native screensChanged with no command queued produces a second
        // commandless OBSERVE capture; the discover auto flight (rev 0) is
        // already settled and must never be re-answered.
        h.fire("screensChanged");
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 2, "one native event sends exactly one observe flight");
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const src = req["source"] as string;
        const nwinv = req["nativeInvalid"] as string;
        assert.ok(typeof src === "string" && src.length > 0, "auto capture has a source");
        // Native membership changes between the send boundary and the
        // accepted apply: WIN_A flips to all-desktops after its SENT
        // snapshot was taken, so sent rows must show the old homing while
        // fresh rows show the new one. No signal fires (direct mutation),
        // so the capture watermark still binds the reply.
        h.wins[0]!.onAllDesktops = true;
        // Held transfer: no actuation, but the desired target still rides
        // the row as held=1 so a native pullback (no transfer) and a held
        // core transfer stay distinguishable.
        answerDescribe(h, req, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
            workspaces: [
                { id: WS, out: OUT, bind: WS, scope: { kind: "workspace", out: OUT, ws: WS }, order: 0, mode: "tiled", exist: true, hold: "" },
            ],
            transfers: [
                { op: "op-held-1", kind: "window", win: WIN_A, out: OUT, ws: WS, follow: false, held: true, cmdSeq: null },
            ],
        });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        // Scope to the second flight: the settled discover auto flight
        // (rev 0) keeps its own rows under its own source.
        const lines = observeLines(h).filter((l) => l.includes(`src=${src}`));
        assert.ok(lines.length > 0, "observe flight leaves observe diagnostics");
        // Every observe row shares the capture source; SENT rows carry no
        // rev (nothing accepted yet) while apply-side rows carry rev=1.
        const srcs = new Set(lines.map(srcOf));
        assert.deepEqual([...srcs], [src], `every observe row shares the capture source: ${lines.join("\n")}`);
        const sent = lines.filter((l) => l.includes("ev=observe-sent "));
        assert.equal(sent.length, 1, `one sent receipt per capture: ${sent.join("\n")}`);
        assert.ok(sent[0]!.includes("snap=sent"), `sent receipt is labelled sent: ${sent[0]}`);
        assert.ok(sent[0]!.includes(`nwinv=${nwinv}`), `sent receipt carries the capture watermark: ${sent[0]}`);
        assert.ok(sent[0]!.includes("cmds=0"), `commandless flight shows zero commands: ${sent[0]}`);
        assert.ok(!sent[0]!.includes("rev="), `sent receipt carries no rev (nothing accepted): ${sent[0]}`);
        assert.ok(sent[0]!.includes("wins=2/2"), `sent receipt names the exact live set: ${sent[0]}`);
        assert.ok(sent[0]!.includes("focus=win-a"), `sent receipt carries the sent focus: ${sent[0]}`);
        assert.ok(sent[0]!.includes("native=1:out:cur=ws1/n=1:order=ws1"),
            `sent receipt bounds native order/current: ${sent[0]}`);
        const sentWins = lines.filter((l) => l.includes("ev=observe-sent-win"));
        assert.equal(sentWins.length, 2, `one sent row per live window: ${sentWins.join("\n")}`);
        assert.ok(sentWins.every((l) => l.includes("snap=sent") && !l.includes("rev=")),
            `sent rows never borrow the later rev: ${sentWins.join("\n")}`);
        assert.ok(sentWins.some((l) => l.includes(`win=${WIN_A}`) && l.includes(`out=${OUT}`) && l.includes(`ws=${WS}`) &&
            l.includes("native=ws1")),
            `sent row carries the pre-change homing: ${sentWins.join("\n")}`);
        const applied = lines.filter((l) => l.includes("ev=observe-applied"));
        assert.equal(applied.length, 1, `one apply receipt per accept: ${applied.join("\n")}`);
        assert.ok(applied[0]!.includes("snap=fresh") && applied[0]!.includes("rev=1"),
            `apply receipt is labelled fresh with the reply rev: ${applied[0]}`);
        assert.ok(applied[0]!.includes(`nwinv=${nwinv}`), `apply receipt repeats the capture watermark: ${applied[0]}`);
        assert.ok(applied[0]!.includes("focusFresh=win-a") && applied[0]!.includes("focusPost=win-a"),
            `apply receipt carries fresh and post-apply focus distinctly: ${applied[0]}`);
        assert.ok(applied[0]!.includes("replyPlans=2") && applied[0]!.includes("replyBinds=1") && applied[0]!.includes("replyXfers=1"),
            `apply receipt counts the reply evidence: ${applied[0]}`);
        const fresh = lines.filter((l) => l.includes("ev=observe-fresh-win"));
        assert.ok(fresh.some((l) => l.includes(`win=${WIN_A}`) && l.includes("snap=fresh") && l.includes("rev=1") &&
            l.includes("native=all")),
            `fresh row shows the post-change all-desktops homing, never merged with sent: ${fresh.join("\n")}`);
        assert.ok(!fresh.some((l) => l.includes(`win=${WIN_A}`) && l.includes("native=ws1")),
            `fresh rows never repeat the stale sent homing: ${fresh.join("\n")}`);
        const plans = lines.filter((l) => l.includes("ev=observe-plan") && !l.includes("omit="));
        assert.ok(plans.every((l) => l.includes("snap=reply")),
            `plan rows are desired-reply evidence, not fresh observations: ${plans.join("\n")}`);
        assert.ok(plans.some((l) => l.includes(`win=${WIN_A}`) && l.includes(`out=${OUT}`) && l.includes(`ws=${WS}`) &&
            l.includes("member=untouched/-") && l.includes("frame=untouched/-")),
            `plan row carries the reply target plus authority: ${plans.join("\n")}`);
        const binds = lines.filter((l) => l.includes("ev=observe-bind") && !l.includes("omit="));
        assert.ok(binds.every((l) => l.includes("snap=reply")),
            `bind rows are desired-reply evidence: ${binds.join("\n")}`);
        assert.ok(binds.some((l) => l.includes(`ws=${WS}`) && l.includes(`out=${OUT}`) && l.includes(`bind=${WS}`)),
            `bind row carries the native binding: ${binds.join("\n")}`);
        const xfers = lines.filter((l) => l.includes("ev=observe-transfer") && !l.includes("omit="));
        assert.equal(xfers.length, 1, `one transfer row: ${xfers.join("\n")}`);
        assert.ok(xfers[0]!.includes("snap=reply") && xfers[0]!.includes("op=op-held-1") && xfers[0]!.includes("kind=window") &&
            xfers[0]!.includes(`win=${WIN_A}`) && xfers[0]!.includes("held=1") && xfers[0]!.includes("rev=1"),
            `held transfer names target and held disposition: ${xfers[0]}`);
        // False absence: the settled auto flight carried zero transfers,
        // so its header counts zero with no transfer rows at all.
        const autoSrc = (JSON.parse(descs[0]!.payload) as Record<string, unknown>)["source"] as string;
        const autoApplied = h.logs.filter((l) => l.includes("ev=observe-applied") && l.includes(`src=${autoSrc}`));
        assert.equal(autoApplied.length, 1, `auto flight leaves one apply receipt: ${autoApplied.join("\n")}`);
        assert.ok(autoApplied[0]!.includes("replyXfers=0"),
            `zero transfers ride the header count: ${autoApplied[0]}`);
        assert.ok(!h.logs.some((l) => l.includes("ev=observe-transfer") && l.includes(`src=${autoSrc}`)),
            "zero transfers leave no transfer rows");
        // The existing command-subject path still exits honestly: no
        // subjects exist on a commandless flight.
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:trace ev=no-subject") && l.includes(`src=${src}`)),
            "commandless flight keeps its no-subject exit");
        h.handle.stop();
    });

    it("observe evidence stays sanitized, bounded and metadata-free", () => {
        const h = makeHarness();
        discover(h);
        const LONG = `win.dotted_under-${"x".repeat(83)}`;
        assert.equal(LONG.length, 100, "long id is ~100 chars");
        h.wins[0]!.internalId = LONG;
        h.setActive(LONG);
        // Second commandless flight (see previous test): the discover auto
        // flight is already settled.
        h.fire("screensChanged");
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 2, "one native event sends exactly one observe flight");
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const src = req["source"] as string;
        // Ten plan/transfer rows against caps of eight: the ninth and tenth
        // must ride explicit omit counts, never silent absence or a longer
        // row. Wire transfer ids stay opaque-safe (no invented fields:
        // WorldReplyTransfer has no source/target arms).
        const windows: Record<string, unknown>[] = [winRow(LONG), winRow(WIN_B)];
        const transfers: Record<string, unknown>[] = [];
        for (let i = 3; i <= 10; i += 1) {
            windows.push(winRow(`win-${i}`));
            transfers.push({ op: `op-${i}`, kind: "window", win: `win-${i}`, out: OUT, ws: WS, follow: false, held: true, cmdSeq: null });
        }
        transfers.push(
            { op: "op-1", kind: "window", win: LONG, out: OUT, ws: WS, follow: false, held: true, cmdSeq: null },
            { op: "op-2", kind: "window", win: WIN_B, out: OUT, ws: WS, follow: false, held: true, cmdSeq: null },
        );
        const workspaces: Record<string, unknown>[] = [
            { id: WS, out: OUT, bind: WS, scope: { kind: "workspace", out: OUT, ws: WS }, order: 0, mode: "tiled", exist: true, hold: "" },
        ];
        for (let i = 2; i <= 10; i += 1) {
            workspaces.push({ id: `ws-${i}`, out: OUT, bind: "", scope: { kind: "workspace", out: OUT, ws: `ws-${i}` }, order: i, mode: "tiled", exist: false, hold: "" });
        }
        answerDescribe(h, req, "1", { windows, workspaces, transfers });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        const lines = observeLines(h).filter((l) => l.includes(`src=${src}`));
        const plans = lines.filter((l) => l.includes("ev=observe-plan") && !l.includes("omit="));
        assert.equal(plans.length, 8, `plan rows cap at eight: ${plans.join("\n")}`);
        assert.ok(plans.every((l) => l.includes("snap=reply")),
            `plan rows are desired-reply evidence: ${plans.join("\n")}`);
        const planOmit = lines.filter((l) => l.includes("ev=observe-plan") && l.includes("omit="));
        assert.equal(planOmit.length, 1, `one plan omit line: ${planOmit.join("\n")}`);
        assert.ok(planOmit[0]!.includes("omit=2"), `plan omit counts the two dropped rows: ${planOmit[0]}`);
        const xfers = lines.filter((l) => l.includes("ev=observe-transfer") && !l.includes("omit="));
        assert.equal(xfers.length, 8, `transfer rows cap at eight: ${xfers.join("\n")}`);
        assert.ok(xfers.every((l) => l.includes("snap=reply")),
            `transfer rows are desired-reply evidence: ${xfers.join("\n")}`);
        const xferOmit = lines.filter((l) => l.includes("ev=observe-transfer") && l.includes("omit="));
        assert.equal(xferOmit.length, 1, `one transfer omit line: ${xferOmit.join("\n")}`);
        assert.ok(xferOmit[0]!.includes("omit=2"), `transfer omit counts the two dropped rows: ${xferOmit[0]}`);
        const bindOmit = lines.filter((l) => l.includes("ev=observe-bind") && l.includes("omit="));
        assert.equal(bindOmit.length, 1, `one bind omit line: ${bindOmit.join("\n")}`);
        assert.ok(bindOmit[0]!.includes("omit=2"), `bind omit counts the two dropped rows: ${bindOmit[0]}`);
        // Identity readability: the full dotted/underscored/long id rides
        // every observe row intact, never collapsed to "-".
        assert.ok(lines.some((l) => l.includes("ev=observe-sent-win") && l.includes(`win=${LONG}`)),
            `sent row keeps the full long id: ${lines.join("\n")}`);
        assert.ok(lines.some((l) => l.includes("ev=observe-plan") && l.includes(`win=${LONG}`)),
            `plan row keeps the full long id: ${lines.join("\n")}`);
        assert.ok(!lines.some((l) => l.includes(`win=${LONG}`) && l.includes("win=-")),
            "long id is never collapsed");
        // Metadata-free: no titles, app names, process ids, roles, geometry
        // dumps or physical output metadata on any observe row.
        const FORBIDDEN = ["manufacturer", "appId", "processId", "transientFor", "resourceClass",
            "clientGeometry", "frameGeometry", "windowType", "title=", "role=", "serialNumber"];
        for (const line of lines) {
            for (const token of FORBIDDEN) {
                assert.ok(!line.includes(token), `observe row leaks ${token}: ${line}`);
            }
        }
        h.handle.stop();
    });

    it("relevant binding beyond the cap is prioritized over irrelevant rows", () => {
        const h = makeHarness();
        discover(h);
        h.fire("screensChanged");
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 2, "one native event sends exactly one observe flight");
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const src = req["source"] as string;
        // Nine irrelevant bindings precede the relevant one: a naive first-8
        // slice would omit the only binding referenced by the shown plan and
        // transfer rows. Prioritization must surface it within the cap.
        const workspaces: Record<string, unknown>[] = [];
        for (let i = 1; i <= 9; i += 1) {
            workspaces.push({ id: `ws-x${i}`, out: OUT, bind: "", scope: { kind: "workspace", out: OUT, ws: `ws-x${i}` }, order: i, mode: "tiled", exist: false, hold: "" });
        }
        workspaces.push({ id: WS, out: OUT, bind: WS, scope: { kind: "workspace", out: OUT, ws: WS }, order: 9, mode: "tiled", exist: true, hold: "" });
        answerDescribe(h, req, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
            workspaces,
            transfers: [
                { op: "op-rel", kind: "window", win: WIN_A, out: OUT, ws: WS, follow: false, held: true, cmdSeq: null },
            ],
        });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        const lines = observeLines(h).filter((l) => l.includes(`src=${src}`));
        const binds = lines.filter((l) => l.includes("ev=observe-bind") && !l.includes("omit="));
        assert.equal(binds.length, 8, `binding rows stay capped at eight: ${binds.join("\n")}`);
        assert.ok(binds.some((l) => l.includes(`ws=${WS}`) && l.includes(`out=${OUT}`) && l.includes(`bind=${WS}`)),
            `relevant trailing binding renders exactly: ${binds.join("\n")}`);
        const bindOmit = lines.filter((l) => l.includes("ev=observe-bind") && l.includes("omit="));
        assert.equal(bindOmit.length, 1, `one bind omit line: ${bindOmit.join("\n")}`);
        assert.ok(bindOmit[0]!.includes("omit=2"), `bind omit preserves the actual dropped count: ${bindOmit[0]}`);
        h.handle.stop();
    });

    it("accepted window transfer records pre/post membership-attempt evidence", () => {
        // Two-desktop single-output world: the transfer moves WIN_A across
        // native desktops without an output send, so setMembership actuates
        // on the fake native and records pre/post setter evidence.
        const WS2 = "ws2";
        const h = makeHarness([WS2]);
        discover(h);
        // Second commandless flight carries the transfer reply; the
        // discover auto flight is already settled.
        h.fire("screensChanged");
        h.runTimers();
        const descs = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(descs.length, 2, "one native event sends exactly one observe flight");
        const req = JSON.parse(descs[descs.length - 1]!.payload) as Record<string, unknown>;
        const src = req["source"] as string;
        h.lastCall("DescribeWorld").callback(
            acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                windows: [
                    winRow(WIN_A, { out: OUT, ws: WS2, floating: false, frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                    winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
                ],
                workspaces: [
                    { id: WS, out: OUT, bind: WS, scope: { kind: "workspace", out: OUT, ws: WS }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: WS2, out: OUT, bind: WS2, scope: { kind: "workspace", out: OUT, ws: WS2 }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                transfers: [
                    { op: "op-move-1", kind: "window", win: WIN_A, out: OUT, ws: WS2, follow: false, held: false, cmdSeq: null },
                ],
            }),
        );
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        // Native proof the transfer actuated before asserting evidence.
        assert.deepEqual(
            h.wins[0]!.desktops.map((d) => d.id),
            [WS2],
            "transfer moved the live native homing",
        );
        const lines = observeLines(h).filter((l) =>
            /ev=(observe-transfer|member-attempt)/.test(l) && l.includes(`src=${src}`));
        const xfer = lines.filter((l) => l.includes("ev=observe-transfer"));
        assert.ok(xfer.some((l) => l.includes("op=op-move-1") && l.includes(`win=${WIN_A}`) &&
            l.includes(`out=${OUT}`) && l.includes(`ws=${WS2}`) && l.includes("held=0") && l.includes("rev=1")),
            `transfer row names the desired target as live: ${xfer.join("\n")}`);
        const members = lines.filter((l) => l.includes("ev=member-attempt") && l.includes(`win=${WIN_A}`));
        assert.ok(members.length >= 1, `transfer leaves membership-attempt evidence: ${members.join("\n")}`);
        assert.ok(members.some((l) => l.includes("setter=accepted") &&
            l.includes(`pre=native/${OUT}/${WS}`) && l.includes(`post=native/${OUT}/${WS2}`)),
            `attempt carries exact pre/post setter evidence: ${members.join("\n")}`);
        assert.ok(members.every((l) => (l.includes("snap=sent") || l.includes("snap=apply-retained")) && l.includes("src=")),
            `attempt rows always scope source under a snap label: ${members.join("\n")}`);
        h.handle.stop();
    });
});

describe("world trace subject-basis correction", () => {
    it("ordinary float conversion resolves take win B, press A never resolved", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestFloat();
        h.runTimers();
        const queued = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = queued["clientCmd"] as string;
        const wire = (queued as Record<string, unknown>)["command"] as Record<string, unknown> | undefined;
        // Production conversion path: press id A queues the closed marker.
        assert.deepEqual(wire?.["win"], { currentFocus: true }, `ordinary press queues the marker: ${JSON.stringify(wire)}`);
        const req = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req, "1", {
            windows: [
                winRow(WIN_A, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_A], alloc: [...TILE_A] }),
                winRow(WIN_B, { floating: false, frame_auth: "untouched", frame_hold: "", frame: [...TILE_B], alloc: [...TILE_B] }),
            ],
            takeMap: { [cmd]: { op: "toggle-float", win: WIN_B, rect: null } },
        });
        h.runTimers();
        ackWriteIfPending(h);
        h.runTimers();
        const cmds = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-float"));
        assert.equal(cmds.length, 1, `one float press leaves one ev=cmd: ${cmds.join("\n")}`);
        assert.ok(cmds[0]!.includes(`press=${WIN_A}`), `cmd retains original press id: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes("win=currentFocus"), `cmd retains actual submitted marker: ${cmds[0]}`);
        const subjects = traceLines(h, "subject");
        assert.ok(subjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes(`win=${WIN_B}`) && l.includes("basis=take")),
            `take win B is the resolved acting subject: ${subjects.join("\n")}`);
        assert.ok(!subjects.some((l) => l.includes(`win=${WIN_A}`) && l.includes("basis=take")),
            `press id A is never labeled resolved: ${subjects.join("\n")}`);
        assert.ok(!subjects.some((l) => l.includes(`win=${WIN_A}`) && l.includes("basis=submitted")),
            `press id A is never labeled submitted: ${subjects.join("\n")}`);
        h.handle.stop();
    });

    it("ordinary sticky conversion with no resolving result stays unassigned plus press-basis", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestSticky();
        h.runTimers();
        const queued = JSON.parse(h.lastCall("QueueWorldCommand").payload) as Record<string, unknown>;
        const cmd = queued["clientCmd"] as string;
        const wire = (queued as Record<string, unknown>)["command"] as Record<string, unknown> | undefined;
        assert.deepEqual(wire?.["win"], { currentFocus: true }, `ordinary sticky queues the marker: ${JSON.stringify(wire)}`);
        const req = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        answerDescribe(h, req, "1", {
            windows: [winRow(WIN_A, { state: [st("sticky", true, "controlled", "", "1"), st("above", true, "controlled", "", "1")] })],
        });
        answerOldestWrite(h);
        h.runTimers();
        const cmds = traceLines(h, "cmd").filter((l) => l.includes("op=toggle-sticky"));
        assert.equal(cmds.length, 1, `one sticky press leaves one ev=cmd: ${cmds.join("\n")}`);
        assert.ok(cmds[0]!.includes(`press=${WIN_A}`), `cmd retains original press id: ${cmds[0]}`);
        assert.ok(cmds[0]!.includes("win=currentFocus"), `cmd retains actual submitted marker: ${cmds[0]}`);
        const un = traceLines(h, "marker-unassigned").filter((l) => l.includes(`clientCmd=${cmd}`));
        assert.equal(un.length, 1, `marker stays unassigned without a take win: ${traceLines(h, "marker-unassigned").join("\n")}`);
        const subjects = traceLines(h, "subject");
        assert.ok(subjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes(`win=${WIN_A}`) && l.includes("basis=press")),
            `press window rides an explicitly press-basis row: ${subjects.join("\n")}`);
        assert.ok(!subjects.some((l) => l.includes(`clientCmd=${cmd}`) && l.includes("basis=take")),
            `no guessed take resolution: ${subjects.join("\n")}`);
        assert.ok(subjects.some((l) => l.includes(`win=${WIN_A}`) && l.includes("basis=press") && l.includes("core=") && l.includes("obs=") && l.includes("fact=")),
            `press-basis row keeps core/obs/fact: ${subjects.join("\n")}`);
        h.handle.stop();
    });
});
