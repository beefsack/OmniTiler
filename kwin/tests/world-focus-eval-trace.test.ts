// Trace-only retained focus evaluation diagnostics: every actual fresh
// evaluation exposes retained (id, target, issuingRev, cause, cmdSeq),
// publication correlation (req, rev, obs), and decision/reason, while
// null/duplicate-spent/unchanged-deferred stay silent and ordinary
// (__OMNITILER_TRACE__="0") builds strip the trace code completely.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createContext, runInContext } from "node:vm";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

type Obs = ApplyEnv["observation"];

const winRow = (winId: string, ws = "ws1", frame: [number, number, number, number] | null = null): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame,
    frame_auth: "controlled", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    frame_op: "",
    clear_op: "",
    floating: false, float_origin: "",
    cmdSeq: null,
    visibility: null, visibility_auth: "untouched", visibility_hold: "",
    state: [],
});

const focusRow = (id: string, target: string, rev: string, opts?: {
    cause?: string; out?: string; ws?: string; cmdSeq?: string | null;
    afterShown?: Record<string, unknown> | null;
}): Record<string, unknown> => ({
    id,
    target,
    rev,
    cause: opts?.cause ?? "positive-close",
    out: opts?.out ?? "out",
    ws: opts?.ws ?? "ws1",
    cmdSeq: opts?.cmdSeq !== undefined ? opts.cmdSeq : null,
    afterShown: opts?.afterShown !== undefined ? opts.afterShown : null,
});

const afterShownFor = (target: string, obs = "1"): Record<string, unknown> => ({
    op: "op-1", obs,
    members: [{ win: target, out: "out", ws: "ws1", frame: null }],
    views: [{ out: "out", ws: "ws1" }],
    selected: [{ out: "out", ws: "ws1" }],
    departing: [],
});

const wsBindRow = (ws = "ws1"): Record<string, unknown> => ({
    id: ws, out: "out", bind: ws === "ws1" ? "native-1" : `native-${ws}`,
    scope: { kind: "output", out: "out" },
    order: 0, mode: "tiled", exist: true, hold: "",
});

function replyText(opts: {
    focusRequest: Record<string, unknown> | null;
    windows?: Record<string, unknown>[];
    gen?: string;
    req?: string;
    obs?: string;
    rev?: string;
}): string {
    return JSON.stringify({
        v: 4, adapter: "test-adapter", gen: opts.gen ?? "gen-1", req: opts.req ?? "req-1",
        obs: opts.obs ?? "1", rev: opts.rev ?? "7", ready: true, consumed: false,
        source: opts.req ?? "req-1", nativeInvalid: "1", clock: "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentDesired: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [wsBindRow("ws1")], outputs: [],
        windows: opts.windows ?? [winRow("win-a"), winRow("win-b")],
        transfers: [],
        effects: [],
        focusRequest: opts.focusRequest,
        result: null,
    });
}

function acceptParsed(text: string): ApplyEnv["reply"] {
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed));
    if (!isCoreWorldReply(parsed)) {
        throw new Error("unreachable");
    }
    return parsed;
}

function makeObs(refs: Map<string, object>, winIds: string[], opts?: {
    gesture?: boolean; focusedGlobal?: string; foregroundKnown?: boolean;
}): Obs {
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["native-1"], current: "native-1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: "native-1", bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "", readable: true, hold: "",
            complete: true,
            windows: winIds.map((win) => ({
                win, ref: refs.get(win) as object,
                out: "out", ws: "native-1",
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false,
                fullscreen: false, maximized: false, sticky: false,
                fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            })),
            closed: [],
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: opts?.gesture ?? false,
        focusedGlobal: opts?.focusedGlobal ?? "win-a",
        foregroundKnown: opts?.foregroundKnown ?? true,
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

function setupState(): WorldAdapterState {
    const state = new WorldAdapterState("test-adapter", "owner-a");
    state.onDiscovery("gen-1", "test-adapter");
    state.invalidate();
    const flight = state.beginCapture("req-1", null);
    assert.ok(flight !== null);
    return state;
}

function makeEnv(
    state: WorldAdapterState,
    observation: Obs,
    reply: ApplyEnv["reply"],
    refs: Map<string, object>,
    logs: string[],
    fg: { current: string | null },
    overrides?: Partial<ApplyEnv>,
): ApplyEnv {
    return {
        state,
        observation,
        reply,
        dispatchWatermark: state.currentInvalid,
        takeResult: null,
        log: (msg) => { logs.push(msg); },
        isStillListed: () => true,
        readActiveWindowId: () => fg.current,
        readWindowId: (ref) => {
            for (const [id, r] of refs) {
                if (r === ref) {
                    return id;
                }
            }
            return null;
        },
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: (ref) => {
            for (const d of observation.domains) {
                for (const w of d.windows) {
                    if (w.ref === ref) {
                        return { out: w.out, ws: w.ws };
                    }
                }
            }
            return null;
        },
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
        setGeometry: () => true,
        setMembership: () => true,
        sendToOutput: () => true,
        resolveOutput: () => ({ out: "out" }),
        resolveDesktop: () => ({ ws: "native-1" }),
        switchView: () => true,
        createDesktop: () => undefined,
        removeDesktop: () => true,
        focusWindow: () => true,
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: () => true,
        readLiveView: () => "native-1",
        desktopById: () => ({ ws: "native-1" }),
        minDesktops: 2,
        ...overrides,
    };
}

function evalLogs(logs: string[]): string[] {
    return logs.filter((l) => l.includes("omnitiler:world:trace ev=focus-eval"));
}

describe("retained focus evaluation trace diagnostics", () => {
    it("logs deferred, superseded, equality, no-call, and called branches with retained and publication fields", () => {
        const refs = new Map<string, object>([["win-a", {}], ["win-b", {}]]);

        // 1. Deferred (afterShown not-ready) with explicit command cmdSeq
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    req: "req-10", obs: "4", rev: "9",
                    focusRequest: focusRow("focus-7", "win-b", "7", {
                        cause: "explicit-command", cmdSeq: "11", afterShown: afterShownFor("win-b", "4"),
                    }),
                })),
                refs,
                logs,
                { current: "win-a" },
                { readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: false }) },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /gen=gen-1 req=req-10 rev=9 obs=4 id=focus-7 target=win-b issuingRev=7 cause=explicit-command cmdSeq=11 decision=deferred reason=not-ready/);
        }

        // 2. Selection readiness stopped -> superseded
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    req: "req-11", obs: "4", rev: "9",
                    focusRequest: focusRow("focus-7", "win-b", "7", {
                        cause: "explicit-command", cmdSeq: "12", afterShown: afterShownFor("win-b", "4"),
                    }),
                })),
                refs,
                logs,
                { current: "win-a" },
                { readLiveRect: () => { state.invalidate(); return null; } },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /req=req-11 rev=9 obs=4 id=focus-7 target=win-b issuingRev=7 cause=explicit-command cmdSeq=12 decision=no-call reason=superseded/);
        }

        // 3. Identity stale (target absent)
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7", { cause: "automatic-admission" }),
                })),
                refs,
                logs,
                { current: "win-a" },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /id=focus-7 target=win-b issuingRev=7 cause=automatic-admission cmdSeq=- decision=no-call reason=identity-stale/);
        }

        // 4. Gesture barrier -> scope-held
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { gesture: true, focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7"),
                })),
                refs,
                logs,
                { current: "win-a" },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /decision=no-call reason=scope-held/);
        }

        // 4b. Unreadable safety -> decision=no-call reason=unreadable
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7"),
                })),
                refs,
                logs,
                { current: "win-a" },
                { readLiveSafety: () => null },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /decision=no-call reason=unreadable/);
        }

        // 5. Silent equality (native already at target)
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-b" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7"),
                })),
                refs,
                logs,
                { current: "win-b" },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /decision=equality reason=already-target/);
        }

        // 6. Positive-close focus-unproven and foreground-changed
        for (const [fgNow, fgKnown, expectedReason] of [
            [null, true, "focus-unproven"],
            ["win-a", false, "focus-unproven"],
            ["win-c", true, "foreground-changed"],
        ] as const) {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a", foregroundKnown: fgKnown }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7", { cause: "positive-close" }),
                })),
                refs,
                logs,
                { current: fgNow },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, new RegExp(`decision=no-call reason=${expectedReason}`));
        }

        // 7. Setter outcomes (setter-accepted, threw, native-refused, focus-unproven)
        for (const [outcome, expectedReason] of [
            ["accepted", "setter-accepted"],
            ["threw", "threw"],
            ["refused", "native-refused"],
            ["unproven", "focus-unproven"],
        ] as const) {
            const state = setupState();
            const logs: string[] = [];
            const fg = { current: "win-a" };
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7"),
                })),
                refs,
                logs,
                fg,
                {
                    focusWindow: () => {
                        if (outcome === "threw") {
                            throw new Error("boom");
                        }
                        if (outcome === "refused") {
                            return false;
                        }
                        if (outcome === "accepted") {
                            fg.current = "win-b";
                        }
                        return true;
                    },
                },
            );
            applyWorldReply(env);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, new RegExp(`decision=called reason=${expectedReason}`));
        }

        // 8. Outer applyWorldReply stoppedMidApply catch-all
        {
            const state = setupState();
            const logs: string[] = [];
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({
                    focusRequest: focusRow("focus-7", "win-b", "7"),
                    windows: [
                        { ...winRow("win-a"), frame_op: "op-1", frame: [0, 0, 200, 200], floating: true },
                        winRow("win-b"),
                    ],
                })),
                refs,
                logs,
                { current: "win-a" },
                { readLiveRect: () => { state.invalidate(); return { x: 0, y: 0, w: 100, h: 100 }; } },
            );
            const res = applyWorldReply(env);
            assert.equal(res.stoppedMidApply, true);
            const lines = evalLogs(logs);
            assert.equal(lines.length, 1);
            assert.match(lines[0]!, /decision=no-call reason=superseded/);
        }
    });

    it("dedups unchanged deferred evaluations and stays silent on null or duplicate-spent envelopes", () => {
        const refs = new Map<string, object>([["win-a", {}], ["win-b", {}]]);
        const state = setupState();
        const logs: string[] = [];
        let visible = false;
        const fg = { current: "win-a" as string | null };

        const runWith = (reqId: string, rev: string, focusRequest: Record<string, unknown> | null): void => {
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({ req: reqId, obs: "1", rev, focusRequest })),
                refs,
                logs,
                fg,
                {
                    readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible }),
                    focusWindow: () => { fg.current = "win-b"; return true; },
                },
            );
            applyWorldReply(env);
        };

        const reqObj = focusRow("focus-7", "win-b", "7", {
            cause: "explicit-command", cmdSeq: "11", afterShown: afterShownFor("win-b", "1"),
        });

        // First deferred evaluation logs once.
        runWith("req-1", "7", reqObj);
        assert.equal(evalLogs(logs).length, 1);

        // Unchanged deferred evaluations on newer req/rev do not spam.
        runWith("req-2", "8", reqObj);
        runWith("req-3", "9", reqObj);
        assert.equal(evalLogs(logs).length, 1);

        // Transitioning to ready + called emits the new outcome.
        visible = true;
        runWith("req-4", "10", reqObj);
        assert.equal(evalLogs(logs).length, 2);
        assert.match(evalLogs(logs)[1]!, /req=req-4 rev=10 .* decision=called reason=setter-accepted/);

        // Re-observing spent request stays silent (duplicate-spent).
        runWith("req-5", "11", reqObj);
        assert.equal(evalLogs(logs).length, 2);

        // Null envelope stays silent.
        runWith("req-6", "12", null);
        assert.equal(evalLogs(logs).length, 2);
    });

    it("does not dedup an identical tuple across a new core generation or same-gen fresh handshake", () => {
        const refs = new Map<string, object>([["win-a", {}], ["win-b", {}]]);
        const state = setupState();
        const logs: string[] = [];
        const reqObj = focusRow("focus-7", "win-b", "7", {
            cause: "explicit-command", cmdSeq: "11", afterShown: afterShownFor("win-b", "1"),
        });

        const runGen = (gen: string, reqId: string): void => {
            const env = makeEnv(
                state,
                makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-a" }),
                acceptParsed(replyText({ gen, req: reqId, obs: "1", rev: "7", focusRequest: reqObj })),
                refs,
                logs,
                { current: "win-a" },
                { readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: false }) },
            );
            applyWorldReply(env);
        };

        runGen("gen-1", "req-1");
        assert.equal(evalLogs(logs).length, 1);
        assert.match(evalLogs(logs)[0]!, /gen=gen-1 req=req-1 .* id=focus-7 .* decision=deferred reason=not-ready/);

        // Same-generation authenticated fresh handshake resets focusCurrent.
        state.onDiscovery("gen-1", "test-adapter");
        state.invalidate();
        const flightSameGen = state.beginCapture("req-2", null);
        assert.ok(flightSameGen !== null);
        runGen("gen-1", "req-2");
        assert.equal(evalLogs(logs).length, 2);
        assert.match(evalLogs(logs)[1]!, /gen=gen-1 req=req-2 .* id=focus-7 .* decision=deferred reason=not-ready/);

        // New-generation handshake also emits the fresh evaluation.
        state.onDiscovery("gen-2", "test-adapter");
        state.invalidate();
        const flight2 = state.beginCapture("req-3", null);
        assert.ok(flight2 !== null);
        runGen("gen-2", "req-3");
        assert.equal(evalLogs(logs).length, 3);
        assert.match(evalLogs(logs)[2]!, /gen=gen-2 req=req-3 .* id=focus-7 .* decision=deferred reason=not-ready/);
    });

    it("strips trace evaluation code and emits nothing when __OMNITILER_TRACE__ is 0", () => {
        const outDir = mkdtempSync(join(tmpdir(), "pat-focus-trace-off-"));
        const outFile = join(outDir, "world-apply.js");
        execFileSync(
            "npx",
            [
                "esbuild",
                "src/world-apply.ts",
                "--bundle",
                "--format=iife",
                "--target=es2017",
                "--minify-syntax",
                "--global-name=PatApply",
                '--define:__OMNITILER_TRACE__="0"',
                `--outfile=${outFile}`,
            ],
            { cwd: process.cwd(), stdio: "pipe" },
        );
        const code = readFileSync(outFile, "utf8");
        assert.ok(!code.includes("ev=focus-eval"), "ordinary build must strip ev=focus-eval");
        assert.ok(!code.includes("focusTraceLast"), "ordinary build must strip focusTraceLast");
        assert.ok(!code.includes("traceFocusEval"), "ordinary build must strip traceFocusEval");

        const context = createContext({});
        runInContext(code, context, { filename: outFile });
        const mod = (context as unknown as { PatApply: { applyWorldReply: typeof applyWorldReply } }).PatApply;

        const refs = new Map<string, object>([["win-a", {}], ["win-b", {}]]);
        const state = setupState();
        const logs: string[] = [];
        const env = makeEnv(
            state,
            makeObs(refs, ["win-a", "win-b"], { focusedGlobal: "win-b" }),
            acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-b", "7") })),
            refs,
            logs,
            { current: "win-b" },
        );
        mod.applyWorldReply(env);
        assert.equal(evalLogs(logs).length, 0);
        assert.ok(!logs.some((l) => l.includes("omnitiler:world:trace")));
    });
});
