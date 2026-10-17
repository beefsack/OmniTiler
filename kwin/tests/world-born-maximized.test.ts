// R-MAX-06 born-maximized admission on the world path (legacy parity):
// the clear phase runs ONLY after a ready full reply with a current
// tiled/controlled scope proof (bootstrap never actuates), before unsafe
// geometry application. Covers the validated setting, the held-fullscreen
// exit lane in both modes, adapter-owned eligibility (fixed, fullscreen,
// floating, sticky, floating-first, applied evidence from the previous
// snapshot, prior attempts), at-most-once per safe lifetime with
// explicit-close pruning, and the production entry flow. Offline only:
// scripted fakes, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import {
    WorldBornMaximizedAdmission,
    type WorldBornMaximizedEnv,
} from "../src/world-born-maximized";
import type { WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const BARE = "11111111-2222-4333-8444-555555555555";
const BRACED = "{11111111-2222-4333-8444-555555555555}";

interface Backing {
    mode: number;
    hasMethod: boolean;
    throws: boolean;
    setMaximizeCalls: number;
    fullscreen: boolean;
    fullScreenWrites: number;
}

function makeTrackerEnv(backing: Map<string, Backing>, logs: string[]): {
    env: WorldBornMaximizedEnv;
    invalidations: { count: number };
} {
    const invalidations = { count: 0 };
    const readProp = (value: object, property: string): unknown => {
        try {
            return Reflect.get(value, property);
        } catch (error) {
            void error;
            return undefined;
        }
    };
    const env: WorldBornMaximizedEnv = {
        readProp,
        // Fresh wrappers per enumeration (never ref identity): each call
        // builds a new object over the shared backing state.
        liveRefById: (id: string): object | null => {
            const state = backing.get(id);
            if (state === undefined) {
                return null;
            }
            const wrapper: Record<string, unknown> = {
                get maximizeMode() {
                    return state.mode;
                },
            };
            if (state.hasMethod) {
                wrapper["setMaximize"] = (vertical: boolean, horizontal: boolean): void => {
                    state.setMaximizeCalls += 1;
                    if (state.throws) {
                        throw new Error("native refused");
                    }
                    state.mode = vertical && horizontal ? 3 : 0;
                };
            }
            return wrapper as object;
        },
        invalidate: (): void => {
            invalidations.count += 1;
        },
        log: (message: string): void => {
            logs.push(message);
        },
        token: (value: unknown): string => String(value),
    };
    return { env, invalidations };
}

interface WinFlags {
    maximized?: boolean;
    fullscreen?: boolean;
    floating?: boolean;
    sticky?: boolean;
    fixedAuto?: boolean;
    fixedSuppress?: boolean;
}

function freshObservation(wins: Array<{ win: string; out: string; ws: string; flags?: WinFlags }>): WorldObservation {
    return {
        mode: "per-output-local",
        topology: [],
        bindings: [],
        domains: [{
            id: "out", ws: "ws", bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 8, focused: "", readable: true, hold: "",
            adjacent: {},
            windows: wins.map((w) => ({
                win: w.win, ref: {}, out: w.out, ws: w.ws,
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: w.flags?.floating ?? false,
                fitExcluded: false,
                fullscreen: w.flags?.fullscreen ?? false,
                maximized: w.flags?.maximized ?? false,
                sticky: w.flags?.sticky ?? false,
                fixedAuto: w.flags?.fixedAuto ?? false,
                fixedSuppress: w.flags?.fixedSuppress ?? false,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            })),
            closed: [], complete: true,
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
    };
}

interface ScopeProof {
    scopeAuth?: string;
    wsMode?: string;
    wsHold?: string;
    outHold?: string;
    wsExist?: boolean;
    wsBind?: string;
}

function readyReply(win: string, out: string, ws: string, proof?: ScopeProof): WorldReply {
    const p = proof ?? {};
    return {
        adapter: "test-adapter", gen: "gen-1", req: "req-1", obs: "1", rev: "1",
        ready: true, consumed: false,
        scopes: [{ scope: `${out}/${ws}`, auth: (p.scopeAuth ?? "controlled") as "controlled", reason: "" }],
        workspaces: [{
            id: ws, out, bind: p.wsBind ?? ws, logical: ws, scope: out,
            order: 0, mode: p.wsMode ?? "tiled", exist: p.wsExist ?? true, hold: p.wsHold ?? "",
        }],
        outputs: [{ out, view: ws, mode: "tiled", hold: p.outHold ?? "" }],
        windows: [{
            win, out, ws, alloc: [0, 0, 100, 100], frame: [0, 0, 100, 100],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "", frame_op: "",
            floating: false, float_origin: "",
        }],
        transfers: [], effects: [], result: null,
    };
}

function tiledMaximized(win: string): { win: string; out: string; ws: string; flags: WinFlags } {
    return { win, out: "out", ws: "ws", flags: { maximized: true } };
}

describe("world born-maximized post-ready admission", () => {
    it("clears a tiled-first birth once under the default with fresh proof", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env, invalidations } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const fresh = freshObservation([tiledMaximized(BARE)]);
        assert.equal(admission.runAfterReady(env, fresh, readyReply(BARE, "out", "ws"), null), true);
        assert.equal(backing.get(BARE)?.mode, 0, "native maximize cleared");
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1);
        assert.equal(invalidations.count, 1, "CURRENT watermark invalidated before the call");
        assert.ok(logs.some((line) => line.includes("outcome=issued")), "issued line");
        assert.ok(
            logs.some((line) => line.includes("outcome=invoked-applied")),
            "applied claimed only with readback proof",
        );
        assert.ok(logs.some((line) => line.includes("echo-armed")), "echo armed before call");
        assert.ok(logs.some((line) => line.includes("echo-cleared-no-signal")), "echo cleared");
        assert.equal(admission.pendingEcho, null);
        // At-most-once: the next ready reply carries applied evidence in
        // the previous snapshot, so the restored window never re-clears.
        const prev: WorldReply = readyReply(BARE, "out", "ws");
        assert.equal(
            admission.runAfterReady(env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: {} }]), readyReply(BARE, "out", "ws"), prev),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1, "no repeat");
    });

    it("does not misclassify the birth row in the current reply as a later maximize", () => {
        // First ready reply with a null previous snapshot still clears:
        // the birth row in the CURRENT reply is not applied evidence.
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        assert.equal(
            new WorldBornMaximizedAdmission().runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1);
    });

    it("keeps the overlay with no clear under keep-maximized-overlay", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env, invalidations } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission(() => "keep-maximized-overlay");
        assert.equal(admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null), false);
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(invalidations.count, 0, "no invalidation without a clear");
    });

    it("resolves missing and invalid modes to the tile-on-launch default", () => {
        for (const raw of [undefined, "bogus", "", 0, null]) {
            const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
            const logs: string[] = [];
            const { env } = makeTrackerEnv(backing, logs);
            const admission = new WorldBornMaximizedAdmission(() => raw);
            assert.equal(
                admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
                true,
                `${String(raw)} clears`,
            );
        }
    });

    it("skips fixed, held fullscreen, floating, and sticky births with no writes", () => {
        const cases: Array<{ name: string; flags: WinFlags }> = [
            { name: "fixed-auto", flags: { maximized: true, fixedAuto: true } },
            { name: "fixed-suppress", flags: { maximized: true, fixedSuppress: true } },
            { name: "fullscreen-held", flags: { maximized: true, fullscreen: true } },
            { name: "floating", flags: { maximized: true, floating: true } },
            { name: "sticky", flags: { maximized: true, sticky: true } },
        ];
        for (const kase of cases) {
            const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
            const logs: string[] = [];
            const { env, invalidations } = makeTrackerEnv(backing, logs);
            assert.equal(
                new WorldBornMaximizedAdmission().runAfterReady(
                    env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: kase.flags }]),
                    readyReply(BARE, "out", "ws"), null,
                ),
                false,
                kase.name,
            );
            assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, `${kase.name} writes nothing`);
            assert.equal(invalidations.count, 0, `${kase.name} invalidates nothing`);
        }
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        new WorldBornMaximizedAdmission().runAfterReady(
            env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { maximized: true, fixedAuto: true } }]),
            readyReply(BARE, "out", "ws"), null,
        );
        assert.ok(logs.some((line) => line.includes("outcome=skipped-fixed")), "fixed skip logged");
    });

    it("never clears opaque unknown or held scopes, or floating workspaces", () => {
        const proofs: Array<{ name: string; proof: ScopeProof }> = [
            { name: "held-scope", proof: { scopeAuth: "hold" } },
            { name: "untouched-scope", proof: { scopeAuth: "untouched" } },
            { name: "unknown-scope", proof: { wsBind: "" } },
            { name: "floating-workspace", proof: { wsMode: "floating" } },
            { name: "held-workspace", proof: { wsHold: "unreadable" } },
            { name: "held-output", proof: { outHold: "unreadable" } },
            { name: "removed-row", proof: { wsExist: false } },
        ];
        for (const kase of proofs) {
            const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
            const logs: string[] = [];
            const { env } = makeTrackerEnv(backing, logs);
            assert.equal(
                new WorldBornMaximizedAdmission().runAfterReady(
                    env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws", kase.proof), null,
                ),
                false,
                kase.name,
            );
            assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, `${kase.name}: zero native calls`);
            assert.ok(logs.some((line) => line.includes("outcome=skipped-scope")), `${kase.name} logged`);
        }
        // Opaque unknown: no matching workspace row at all.
        {
            const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
            const logs: string[] = [];
            const { env } = makeTrackerEnv(backing, logs);
            const reply = readyReply(BARE, "out", "ws");
            reply.workspaces = [];
            assert.equal(
                new WorldBornMaximizedAdmission().runAfterReady(env, freshObservation([tiledMaximized(BARE)]), reply, null),
                false,
                "unknown-scope",
            );
            assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "unknown-scope: zero native calls");
        }
    });

    it("keeps floating-first sightings overlaid under the default", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const untiled = readyReply(BARE, "out", "ws", { scopeAuth: "hold" });
        // First sighted floating on an untiled scope, later tiled-but-maximized.
        assert.equal(
            admission.runAfterReady(
                env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { floating: true } }]), untiled, null,
            ),
            false,
        );
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
            "floating-first keeps overlay",
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0);
    });

    it("leaves a later maximize of a previously admitted tile untouched", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const prev: WorldReply = readyReply(BARE, "out", "ws");
        assert.equal(
            new WorldBornMaximizedAdmission().runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), prev),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "applied evidence keeps the overlay");
    });

    it("records the attempt on a refused setter with no repeat and no applied claim", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: false, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
            "issued counts the attempt",
        );
        assert.equal(backing.get(BARE)?.mode, 3, "overlay slot retained");
        assert.ok(logs.some((line) => line.includes("refused-missing")), "refusal logged");
        assert.ok(!logs.some((line) => line.includes("applied")), "no false applied claim");
        logs.length = 0;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
            "at-most-once",
        );
        assert.ok(!logs.some((line) => line.includes("born-maximized-clear")), "silent afterwards");
    });

    it("claims nothing applied when the readback stays maximized", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: true, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        assert.equal(
            new WorldBornMaximizedAdmission().runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.ok(logs.some((line) => line.includes("outcome=threw-unproven")), "throw without proof");
        assert.ok(!logs.some((line) => line.includes("applied")), "no false completed claim");
    });

    it("applies a mode change to subsequent admissions without resetting attempts", () => {
        let mode: unknown = "keep-maximized-overlay";
        const backing = new Map([
            [BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }],
            ["win-c", { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }],
        ]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission(() => mode);
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        mode = "tile-on-launch";
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.equal(backing.get(BARE)?.mode, 0);
        assert.ok(
            logs.some((line) => line === "omnitiler:world:config-reloaded stage=born-maximized mode=tile-on-launch"),
            "reload logged for subsequent admissions",
        );
        mode = "keep-maximized-overlay";
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized("win-c")]), readyReply("win-c", "out", "ws"), null),
            false,
        );
        mode = "tile-on-launch";
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
            "attempts never reset",
        );
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized("win-c")]), readyReply("win-c", "out", "ws"), null),
            true,
            "new birth clears",
        );
        assert.equal(backing.get("win-c")?.mode, 0);
    });

    it("prunes lifetimes only on explicit close, never on minimized absence", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        // Refused setter records the attempt.
        admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null);
        // Minimized absence (no explicit close) prunes nothing.
        admission.pruneClosed([]);
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
            "still at-most-once without explicit close",
        );
        // Explicit close starts a fresh lifetime: eligible again.
        admission.pruneClosed([BARE]);
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
            "id reuse after close is a new lifetime",
        );
    });
});

describe("world born-maximized held-marker consumption", () => {
    function newBacking(mode: number): Map<string, Backing> {
        return new Map([[BARE, { mode, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
    }

    it("consumes a non-maximized held exit so a later maximize never clears", () => {
        for (const bornMode of ["tile-on-launch", "keep-maximized-overlay"] as const) {
            const backing = newBacking(0);
            const logs: string[] = [];
            const { env, invalidations } = makeTrackerEnv(backing, logs);
            const admission = new WorldBornMaximizedAdmission(
                bornMode === "tile-on-launch" ? undefined : () => bornMode,
            );
            const reply = readyReply(BARE, "out", "ws");
            // Born fullscreen: held slotless, no clear under either lane.
            const bornFull = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]);
            assert.equal(admission.runAfterReady(env, bornFull, reply, null), false);
            // First exit is NOT maximized: the held marker is consumed with
            // no writes, so it can never resurrect on a later maximize.
            const exited = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: {} }]);
            assert.equal(admission.runAfterReady(env, exited, reply, reply), false);
            assert.equal(backing.get(BARE)?.setMaximizeCalls, 0);
            // Service accepts the exit (settled evidence, as on the wire).
            admission.noteAcceptedReply(exited, reply);
            // Later user maximize after a generation rebind (null previous
            // snapshot): not a held exit (marker gone) and admitted (kept
            // lifetime), so zero clears under both modes.
            backing.get(BARE)!.mode = 3;
            assert.equal(
                admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
                false,
                `${bornMode}: no held-exit resurrection`,
            );
            assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, `${bornMode}: zero native calls`);
            assert.equal(backing.get(BARE)?.mode, 3, `${bornMode}: overlay kept`);
            assert.equal(invalidations.count, 0, `${bornMode}: never invalidated`);
        }
    });

    it("consumes a fixed-skipped max exit so a later unfixed maximize never clears", () => {
        const backing = newBacking(0);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const reply = readyReply(BARE, "out", "ws");
        admission.runAfterReady(
            env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]), reply, null,
        );
        // Fullscreen exits while still maximized but fixed: skipped with no
        // writes, and the held marker is spent anyway.
        backing.get(BARE)!.mode = 3;
        const fixedExit = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { maximized: true, fixedAuto: true } }]);
        assert.equal(admission.runAfterReady(env, fixedExit, reply, reply), false);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "fixed exits take no writes");
        assert.ok(logs.some((line) => line.includes("outcome=skipped-fixed")), "fixed skip logged");
        admission.noteAcceptedReply(fixedExit, reply);
        // A later predicate change cannot resurrect the held exit: the same
        // lifetime maximizing unfixed still clears nothing.
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
    });
});

describe("world born-maximized settled lifetime across generation rebind", () => {
    function newBacking(): Map<string, Backing> {
        const ids = [BARE, "win-c"];
        return new Map(ids.map((id) => [id, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]));
    }

    it("keeps a keep-mode maximize across a rebind with a null previous snapshot", () => {
        const backing = newBacking();
        const logs: string[] = [];
        const { env, invalidations } = makeTrackerEnv(backing, logs);
        let mode: unknown = "keep-maximized-overlay";
        const admission = new WorldBornMaximizedAdmission(() => mode);
        const freshMax = freshObservation([tiledMaximized(BARE)]);
        const reply = readyReply(BARE, "out", "ws");
        // Born under keep: overlay kept, no clear attempted.
        assert.equal(admission.runAfterReady(env, freshMax, reply, null), false);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0);
        // Service accepts the reply: settled admitted-lifetime evidence.
        admission.noteAcceptedReply(freshMax, reply);
        // Planner generation reset (null previous snapshot) with the
        // setting flipped to the default meanwhile: the kept maximize is
        // admitted, not a new birth, so no native call.
        mode = "tile-on-launch";
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
        assert.equal(invalidations.count, 0, "no invalidation without a clear");
        // A genuinely new id still clears under the default.
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized("win-c")]), readyReply("win-c", "out", "ws"), null),
            true,
        );
        assert.equal(backing.get("win-c")?.mode, 0, "new birth clears");
    });

    it("treats a later maximize of a settled normal birth as admitted after a rebind", () => {
        const backing = newBacking();
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const freshNormal = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: {} }]);
        const reply = readyReply(BARE, "out", "ws");
        assert.equal(admission.runAfterReady(env, freshNormal, reply, null), false);
        admission.noteAcceptedReply(freshNormal, reply);
        // The settled birth maximizes later; after a rebind the null
        // previous snapshot must not reclassify it as a new birth.
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
    });

    it("starts a fresh lifetime after explicit close", () => {
        const backing = newBacking();
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const freshMax = freshObservation([tiledMaximized(BARE)]);
        const reply = readyReply(BARE, "out", "ws");
        assert.equal(admission.runAfterReady(env, freshMax, reply, null), true);
        admission.noteAcceptedReply(freshObservation([{ win: BARE, out: "out", ws: "ws", flags: {} }]), reply);
        // Explicit close prunes every lifetime set: id reuse is a birth.
        admission.pruneClosed([BARE]);
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
            "reused id after close clears again",
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 2, "cleared exactly twice");
    });

    it("never marks unknown or held scopes, or still-held fullscreen births", () => {
        const backing = newBacking();
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        // Fullscreen birth on a held scope: never marked.
        const heldReply = readyReply(BARE, "out", "ws", { scopeAuth: "hold" });
        const bornFull = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]);
        assert.equal(admission.runAfterReady(env, bornFull, heldReply, null), false);
        admission.noteAcceptedReply(bornFull, heldReply);
        // First safe tiled maximize still clears once under the default:
        // the pending birth was never admitted-lifetime evidence.
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.equal(backing.get(BARE)?.mode, 0, "first safe birth clears");
    });
});

describe("world born-maximized held-fullscreen exit lane", () => {
    it("clears a held-fullscreen exit once under the default", () => {
        const backing = new Map([[BARE, { mode: 0, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        // Born fullscreen: held slotless, no clear under either lane.
        assert.equal(
            admission.runAfterReady(
                env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]),
                readyReply(BARE, "out", "ws"), null,
            ),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0);
        // Fullscreen exits while still maximized: exit lane clears once.
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.equal(backing.get(BARE)?.mode, 0);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1);
    });

    it("clears a held-fullscreen exit once under keep-maximized-overlay", () => {
        const backing = new Map([[BARE, { mode: 0, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission(() => "keep-maximized-overlay");
        assert.equal(
            admission.runAfterReady(
                env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]),
                readyReply(BARE, "out", "ws"), null,
            ),
            false,
        );
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
            "exit lane ignores the keep setting",
        );
        assert.equal(backing.get(BARE)?.mode, 0);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1);
    });

    it("writes nothing for a fixed held-fullscreen exit", () => {
        const backing = new Map([[BARE, { mode: 0, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        admission.runAfterReady(
            env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]),
            readyReply(BARE, "out", "ws"), null,
        );
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(
                env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { maximized: true, fixedAuto: true } }]),
                readyReply(BARE, "out", "ws"), null,
            ),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "fixed exits take no writes");
        assert.ok(logs.some((line) => line.includes("outcome=skipped-fixed")), "fixed skip logged");
    });

    it("never marks later fullscreen on admitted tiles as births", () => {
        const backing = new Map([[BARE, { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }]]);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const prev: WorldReply = readyReply(BARE, "out", "ws");
        // Admitted tile goes fullscreen: not a birth, never held.
        assert.equal(
            admission.runAfterReady(
                env, freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true, maximized: true } }]),
                readyReply(BARE, "out", "ws"), prev,
            ),
            false,
        );
        // ...exits still maximized: born lane sees applied evidence, keeps overlay.
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), prev),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "reserved tiles keep their overlay");
    });
});

// ---------- production entry flow: reply -> clear -> apply ----------

interface EntryBacking {
    mode: number;
    fullscreen: boolean;
    setMaximizeCalls: number;
    fullScreenWrites: number;
    fixed: boolean;
}

function makeBornSurface(backing: Map<string, EntryBacking>) {
    const ws1 = { id: "ws1" };
    const signals: Record<string, Array<() => void>> = { windowAdded: [] };
    const signalOf = (name: string): { connect: (h: () => void) => void; disconnect: (h: () => void) => void } => ({
        connect: (h: () => void): void => {
            signals[name]?.push(h);
        },
        disconnect: (h: () => void): void => {
            const handlers = signals[name];
            if (handlers !== undefined) {
                const at = handlers.indexOf(h);
                if (at >= 0) {
                    handlers.splice(at, 1);
                }
            }
        },
    });
    const freshWins = (): Array<Record<string, unknown>> => {
        const out: Array<Record<string, unknown>> = [];
        for (const [id, state] of backing) {
            const record: Record<string, unknown> = {};
            Object.defineProperties(record, {
                internalId: { get: () => BRACED, enumerable: true },
                normalWindow: { value: true, enumerable: true },
                frameGeometry: {
                    value: { x: 0, y: 0, width: 200, height: 200 }, enumerable: true,
                },
                output: { value: { name: "out" }, enumerable: true },
                desktops: { value: [ws1], enumerable: true },
                minimized: { value: false, enumerable: true },
                fullScreen: {
                    get: () => state.fullscreen,
                    set: () => {
                        state.fullScreenWrites += 1;
                    },
                    enumerable: true, configurable: true,
                },
                maximizeMode: {
                    get: () => state.mode, enumerable: true, configurable: true,
                },
                onAllDesktops: { value: false, enumerable: true },
                minSize: { value: state.fixed ? { width: 200, height: 200 } : null, enumerable: true },
                maxSize: { value: state.fixed ? { width: 200, height: 200 } : null, enumerable: true },
            });
            record["setMaximize"] = (vertical: boolean, horizontal: boolean): void => {
                state.setMaximizeCalls += 1;
                state.mode = vertical && horizontal ? 3 : 0;
            };
            void id;
            out.push(record);
        }
        return out;
    };
    const world = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: null as unknown,
        currentDesktop: ws1 as unknown,
    };
    const surface = {
        windowList: () => freshWins(),
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        windowAdded: signalOf("windowAdded"),
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
            world.currentDesktop = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef;
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, signals };
}

function startBornEntry(
    surface: object,
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>,
    timers: Array<{ delay: number; cb: () => void }>,
    logs: string[],
    extra?: {
        readBornMaximizedFn?: () => unknown;
        readDefaultTiledFn?: () => unknown;
    },
) {
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
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        ...(extra?.readBornMaximizedFn !== undefined ? { readBornMaximizedFn: extra.readBornMaximizedFn } : {}),
        ...(extra?.readDefaultTiledFn !== undefined ? { readDefaultTiledFn: extra.readDefaultTiledFn } : {}),
    });
    assert.ok(handle !== null);
    return handle;
}

function discoverBorn(
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>,
): void {
    const discover = calls.find((c) => c.method === "DiscoverWorld");
    assert.ok(discover !== undefined);
    discover.callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen: "gen-1", req: JSON.parse(discover.payload).req,
        obs: "0", rev: "0", ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [],
    }));
}

function lastDescribe(calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>) {
    const found = [...calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(found !== undefined);
    return found as { method: string; payload: string; callback: (reply: unknown) => void };
}

function acceptReady(
    call: { payload: string; callback: (reply: unknown) => void },
    win: string,
    overrides?: { wsMode?: string; scopeAuth?: string; rev?: string },
): void {
    const body = JSON.parse(call.payload) as { req: string; obs: string };
    call.callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen: "gen-1", req: body.req,
        obs: body.obs, rev: overrides?.rev ?? "1", ready: true, consumed: false,
        scopes: [{ scope: "out/ws1", auth: overrides?.scopeAuth ?? "controlled", reason: "" }],
        workspaces: [{
            id: "ws1", out: "out", bind: "ws1", logical: "ws1", scope: "out",
            order: 0, mode: overrides?.wsMode ?? "tiled", exist: true, hold: "",
        }],
        outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
        windows: [{
            win, out: "out", ws: "ws1",
            alloc: [12, 12, 376, 576], frame: [12, 12, 376, 576],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "", frame_op: "",
            floating: false, float_origin: "",
        }],
        transfers: [], effects: [], result: null,
    }));
}

describe("world born-maximized entry flow", () => {
    it("actuates nothing before the first ready reply", () => {
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs);
        // Discovery alone actuates nothing.
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0);
        discoverBorn(calls);
        // The pre-ready auto report actuates nothing: the overlay is
        // reserved until the ready reply confirms the tile.
        const auto = lastDescribe(calls);
        const body = JSON.parse(auto.payload) as {
            domains: Array<{ windows: Array<{ win: string; maximized: boolean }> }>;
        };
        assert.ok(body.domains.flatMap((d) => d.windows).some((w) => w.maximized), "birth rides unproven");
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "zero actuation before bootstrap");
        assert.equal(backing.get("win-b")?.fullScreenWrites, 0, "no fullscreen writes");
        handle?.stop();
    });

    it("clears after the ready tile proof, then applies from recaptured truth without replay", () => {
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs);
        discoverBorn(calls);
        acceptReady(lastDescribe(calls), BARE);
        assert.equal(backing.get("win-b")?.mode, 0, "native overlay cleared after ready proof");
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 1, "cleared exactly once");
        assert.equal(backing.get("win-b")?.fullScreenWrites, 0, "no fullscreen writes");
        assert.ok(logs.some((line) => line.includes("outcome=invoked-applied")), "clear proven");
        // Observation-only catchup was queued: firing it redispatches
        // without any command.
        const catchup = timers.find((t) => t.delay <= 120);
        assert.ok(catchup !== undefined, "catchup redispatched");
        catchup.cb();
        const next = lastDescribe(calls);
        const body = JSON.parse(next.payload) as {
            command: unknown;
            domains: Array<{ windows: Array<{ win: string; maximized: boolean }> }>;
        };
        assert.equal(body.command, null, "no command replay");
        assert.ok(
            body.domains.flatMap((d) => d.windows).some((w) => w.win === BARE && !w.maximized),
            "recaptured truth carries the restored tile",
        );
        handle?.stop();
    });

    it("keeps the overlay under keep-maximized-overlay with no clear", () => {
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs, { readBornMaximizedFn: () => "keep-maximized-overlay" });
        discoverBorn(calls);
        acceptReady(lastDescribe(calls), BARE);
        assert.equal(backing.get("win-b")?.mode, 3, "overlay kept");
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "zero native calls");
        assert.ok(!logs.some((line) => line.includes("born-maximized-clear")), "no clear attempted");
        handle?.stop();
    });

    it("clears nothing on a defaultTiled=false floating scope", () => {
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs, { readDefaultTiledFn: () => false });
        discoverBorn(calls);
        // Floating seed: the workspace row is floating, so the scope
        // cannot prove a tile even though the window is maximized.
        acceptReady(lastDescribe(calls), BARE, { wsMode: "floating" });
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "zero setMaximize calls");
        assert.equal(backing.get("win-b")?.mode, 3, "overlay kept");
        handle?.stop();
    });

    it("clears a held-fullscreen exit once in both modes with no fullscreen writes", () => {
        for (const bornMode of ["tile-on-launch", "keep-maximized-overlay"] as const) {
            const backing = new Map([["win-b", { mode: 0, fullscreen: true, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
            const { surface, signals } = makeBornSurface(backing);
            const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
            const timers: Array<{ delay: number; cb: () => void }> = [];
            const logs: string[] = [];
            const handle = startBornEntry(surface, calls, timers, logs, {
                ...(bornMode === "tile-on-launch" ? {} : { readBornMaximizedFn: () => bornMode }),
            });
            discoverBorn(calls);
            // First reply: born fullscreen held slotless, no clear.
            acceptReady(lastDescribe(calls), BARE);
            assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, `${bornMode}: held fullscreen waits`);
            // Fullscreen exits while still maximized: exit lane clears once.
            backing.get("win-b")!.fullscreen = false;
            backing.get("win-b")!.mode = 3;
            for (const fire of [...(signals.windowAdded ?? [])]) {
                fire();
            }
            const debounce = timers.find((t) => t.delay <= 120);
            assert.ok(debounce !== undefined, `${bornMode}: redispatched`);
            debounce.cb();
            acceptReady(lastDescribe(calls), BARE, { rev: "2" });
            assert.equal(backing.get("win-b")?.mode, 0, `${bornMode}: exit cleared`);
            assert.equal(backing.get("win-b")?.setMaximizeCalls, 1, `${bornMode}: cleared exactly once`);
            assert.equal(backing.get("win-b")?.fullScreenWrites, 0, `${bornMode}: no fullscreen writes`);
            handle?.stop();
        }
    });

    it("writes nothing for a fixed held-fullscreen exit", () => {
        const backing = new Map([["win-b", { mode: 0, fullscreen: true, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: true }]]);
        const { surface, signals } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs);
        discoverBorn(calls);
        acceptReady(lastDescribe(calls), BARE);
        backing.get("win-b")!.fullscreen = false;
        backing.get("win-b")!.mode = 3;
        for (const fire of [...(signals.windowAdded ?? [])]) {
            fire();
        }
        const debounce = timers.find((t) => t.delay <= 120);
        assert.ok(debounce !== undefined);
        debounce.cb();
        acceptReady(lastDescribe(calls), BARE, { rev: "2" });
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "fixed exits take no writes");
        assert.ok(logs.some((line) => line.includes("outcome=skipped-fixed")), "fixed skip logged");
        handle?.stop();
    });

    it("keeps a settled keep-mode maximize across a planner generation reset", () => {
        let bornMode: unknown = "keep-maximized-overlay";
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface, signals } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const handle = startBornEntry(surface, calls, timers, logs, { readBornMaximizedFn: () => bornMode });
        discoverBorn(calls);
        acceptReady(lastDescribe(calls), BARE);
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "keep mode issues no clear");
        // Flip to the default, then force a planner generation reset: the
        // next DescribeWorld gets a generation-mismatch rejection and the
        // adapter re-discovers under a new generation (null previous
        // snapshot from then on).
        bornMode = "tile-on-launch";
        for (const fire of [...(signals.windowAdded ?? [])]) {
            fire();
        }
        const debounce = timers.find((t) => t.delay <= 120);
        assert.ok(debounce !== undefined, "redispatched");
        debounce.cb();
        lastDescribe(calls).callback(JSON.stringify({
            outcome: "rejected", kind: "generation-mismatch", message: "planner restarted", gen: "gen-1",
        }));
        assert.ok(logs.some((line) => line.includes("generation-mismatch")), "reset observed");
        const rediscover = [...calls].reverse().find((c) => c.method === "DiscoverWorld");
        assert.ok(rediscover !== undefined, "re-discovery dispatched");
        const discBody = JSON.parse(rediscover.payload) as { req: string };
        rediscover.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-2", req: discBody.req,
            obs: "0", rev: "0", ready: false, consumed: false,
            scopes: [], workspaces: [], outputs: [], windows: [],
            transfers: [], effects: [],
        }));
        // The post-reset ready reply arrives with a null previous snapshot:
        // the settled keep-mode maximize is admitted, not a new birth, so
        // the default issues no clear.
        const describe = lastDescribe(calls);
        const readyBody = JSON.parse(describe.payload) as { req: string; obs: string };
        describe.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-2", req: readyBody.req,
            obs: readyBody.obs, rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [{
                id: "ws1", out: "out", bind: "ws1", logical: "ws1", scope: "out",
                order: 0, mode: "tiled", exist: true, hold: "",
            }],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            windows: [{
                win: BARE, out: "out", ws: "ws1",
                alloc: [12, 12, 376, 576], frame: [12, 12, 376, 576],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "", frame_op: "",
                floating: false, float_origin: "",
            }],
            transfers: [], effects: [], result: null,
        }));
        assert.equal(backing.get("win-b")?.setMaximizeCalls, 0, "post-reset default issues no clear for the settled maximize");
        assert.equal(backing.get("win-b")?.mode, 3, "overlay kept");
        assert.equal(backing.get("win-b")?.fullScreenWrites, 0, "no fullscreen writes");
        handle?.stop();
    });

    it("braced-UUID births resolve to the canonical id in the report", () => {
        const backing = new Map([["win-b", { mode: 3, fullscreen: false, setMaximizeCalls: 0, fullScreenWrites: 0, fixed: false }]]);
        const { surface } = makeBornSurface(backing);
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        void BRACED;
        const handle = startBornEntry(surface, calls, timers, logs);
        discoverBorn(calls);
        const body = JSON.parse(lastDescribe(calls).payload) as {
            domains: Array<{ windows: Array<{ win: string }> }>;
        };
        assert.ok(
            body.domains.flatMap((d) => d.windows).some((w) => w.win === BARE),
            "braced internalId unwraps to the canonical id",
        );
        handle?.stop();
    });
});

describe("world born-maximized floating-workspace proof", () => {
    function floatingReply(win: string): WorldReply {
        return readyReply(win, "out", "ws", { scopeAuth: "untouched", wsMode: "floating" });
    }

    function floatingBacking(mode: number): Map<string, Backing> {
        return new Map<string, Backing>([
            [BARE, { mode, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }],
            ["win-c", { mode: 3, hasMethod: true, throws: false, setMaximizeCalls: 0, fullscreen: false, fullScreenWrites: 0 }],
        ]);
    }

    it("keeps an ordinary maximize on a floating workspace across a tiled reseed rebind", () => {
        const backing = floatingBacking(3);
        const logs: string[] = [];
        const { env, invalidations } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        // Ordinary member: the workspace floats it, so entry.floating is false.
        const freshMax = freshObservation([tiledMaximized(BARE)]);
        const floatReply = floatingReply(BARE);
        assert.equal(admission.runAfterReady(env, freshMax, floatReply, null), false);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "floating scope never clears");
        admission.noteAcceptedReply(freshMax, floatReply);
        // Planner rebind reseeds the scope tiled with a null previous
        // snapshot: the same lifetime is floating-first and settled, not a
        // new birth, so the default issues no clear.
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
        assert.equal(invalidations.count, 0, "no invalidation without a clear");
        // A genuinely new id on the tiled default still clears.
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized("win-c")]), readyReply("win-c", "out", "ws"), null),
            true,
        );
        assert.equal(backing.get("win-c")?.mode, 0, "new birth clears");
    });

    it("never enters the held lane for a fullscreen birth on a floating workspace", () => {
        const backing = floatingBacking(0);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const floatReply = floatingReply(BARE);
        const bornFull = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]);
        assert.equal(admission.runAfterReady(env, bornFull, floatReply, null), false);
        admission.noteAcceptedReply(bornFull, floatReply);
        // Exit maximized after the scope reseeds tiled: no held marker was
        // ever recorded, so the held lane cannot bypass floating-first and
        // settled safety even with a null previous snapshot.
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
    });

    it("cancels a held marker when the scope proves floating", () => {
        const backing = floatingBacking(0);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const tiledReply = readyReply(BARE, "out", "ws");
        // Fullscreen birth on a tiled scope: held, waiting for its exit.
        const bornFull = freshObservation([{ win: BARE, out: "out", ws: "ws", flags: { fullscreen: true } }]);
        assert.equal(admission.runAfterReady(env, bornFull, tiledReply, null), false);
        // Scope reseeds floating while still fullscreen: the explicit float
        // proof cancels the held marker with no writes.
        assert.equal(admission.runAfterReady(env, bornFull, floatingReply(BARE), tiledReply), false);
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "cancellation writes nothing");
        admission.noteAcceptedReply(bornFull, floatingReply(BARE));
        // Exit maximized after the scope reseeds tiled: the cancelled lane
        // stays dead with a null previous snapshot.
        backing.get(BARE)!.mode = 3;
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            false,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 0, "zero native calls");
        assert.equal(backing.get(BARE)?.mode, 3, "overlay kept");
    });

    it("prunes floating-scope evidence on explicit close", () => {
        const backing = floatingBacking(3);
        const logs: string[] = [];
        const { env } = makeTrackerEnv(backing, logs);
        const admission = new WorldBornMaximizedAdmission();
        const freshMax = freshObservation([tiledMaximized(BARE)]);
        const floatReply = floatingReply(BARE);
        assert.equal(admission.runAfterReady(env, freshMax, floatReply, null), false);
        admission.noteAcceptedReply(freshMax, floatReply);
        // Explicit close prunes floating-first and settled evidence: id
        // reuse on a tiled scope is a birth and clears once.
        admission.pruneClosed([BARE]);
        assert.equal(
            admission.runAfterReady(env, freshObservation([tiledMaximized(BARE)]), readyReply(BARE, "out", "ws"), null),
            true,
        );
        assert.equal(backing.get(BARE)?.setMaximizeCalls, 1, "reused id clears once");
        assert.equal(backing.get(BARE)?.mode, 0, "overlay cleared");
    });
});
