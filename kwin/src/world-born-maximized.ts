// R-MAX-06 born-maximized admission for the world adapter (legacy
// plan-adapter parity, adapter-owned). The clear phase runs ONLY after a
// ready full reply (bootstrap never actuates: `acceptReply` gates
// `ready:false` to observation-only catchup, and this phase runs solely on
// the accepted path), with a current tiled/controlled scope proof from the
// reply (never TS default-policy fallback), BEFORE unsafe geometry
// application.
//
// Two lanes, mirroring `clearMaximizeAtAdmission` plus the held-fullscreen
// exit:
// - Born lane: under the `tile-on-launch` default a tiled-first born
//   maximized window clears once natively so the core-approved allocation
//   applies; `keep-maximized-overlay` keeps the overlay (no clear).
// - Held-fullscreen exit lane (legacy exact): a former held fullscreen
//   birth whose fullscreen first exits while still maximized clears once
//   under BOTH settings unless the CURRENT fixed predicate skips it. No
//   `fullScreen` write ever happens here, only `setMaximize(false,false)`.
//   Later fullscreen on reserved (admitted) tiles is not a birth and never
//   enters the held set.
//
// Eligibility is adapter-owned and documented here (never core policy, no
// TS workspace-policy fallback): opaque unknown scopes (no matching reply
// rows) and held scopes never clear; floating workspaces (e.g. a
// `defaultTiled=false` seed) never clear, proven by the core reply itself
// (untouched scope plus a bound/existing/unheld floating workspace row
// plus an unheld output row), never by TS-side default inference;
// fixed-size births float untouched; floating/sticky identities keep their
// overlay; R-MAX-03 floating-first sightings (floating on an untiled scope,
// plus ordinary `floating=false` members homed on a proven floating
// workspace) keep theirs under both settings; ids with applied evidence in
// the PREVIOUS accepted snapshot are later maximizes that keep their
// overlay (the current new reply contains the birth row and must not
// misclassify it); each canonical native lifetime clears at most once.
// Fullscreen births on a proven floating workspace never enter the held
// set, and a held marker is cancelled when the scope proves floating, so
// the held exit lane can never bypass floating-first safety.
//
// Lifetime sets (`attempts`, floating-first sightings, held-fullscreen,
// settled-admitted) are keyed by canonical native id and pruned ONLY on
// known explicit close (the close-tracker pre-ack snapshot via
// `pruneClosed`): never an arbitrary numeric eviction, never on minimized
// absence (minimized lifetimes are live and absent from domain membership;
// pruning them would re-clear on restore). No numeric-cap boundedness is
// claimed for these sets; explicit-close pruning is the bound.
//
// `settledAdmitted` retains settled/admitted-lifetime evidence across
// Planner generation rebinds: ids the core provably knew in an accepted
// ready reply (recorded via `noteAcceptedReply` AFTER that reply's birth
// decision, so the initial default clear still fires once) stay classified
// as admitted when a later ready reply arrives with a null previous
// snapshot. Only safe actual core controlled OR floating known workspaces
// mark (a current reply row on a tiled/controlled scope proof, or on a
// proven floating-workspace proof for ordinary `floating=false` members);
// unknown/held-scope and still-held fullscreen births never mark, and the
// held exit lane overrides admitted until its marker is consumed.
//
// No gaming, geometry, membership, or view writes happen here: the single
// native call is `setMaximize(false,false)` through the exact KWin
// Q_INVOKABLE, proven through a fresh `maximizeMode` readback on a
// re-resolved live ref (never ref identity, never the setter outcome
// alone). A refused setter still records the attempt (no repeats) and
// keeps the overlay slot for a later maximize. On any issued clear the
// caller invalidates the CURRENT watermark before the call, recaptures
// truth, and continues observation-only catchup (commands never replay).

import { BORN_MAXIMIZED_DEFAULT, readBornMaximizedValue } from "./born-maximized";
import type { WorldReply } from "./world-report";
import type { WorldObservation } from "./world-observe";

export interface WorldBornMaximizedEnv {
    readonly readProp: (value: object, property: string) => unknown;
    readonly liveRefById: (id: string) => object | null;
    readonly log: (message: string) => void;
    readonly token: (value: unknown) => string;
    readonly invalidate: () => void;
}

function readMaximized(readProp: WorldBornMaximizedEnv["readProp"], ref: object): boolean | null {
    try {
        const mode = readProp(ref, "maximizeMode");
        if (typeof mode === "number") {
            return mode !== 0;
        }
        const legacy = readProp(ref, "maximized");
        return typeof legacy === "boolean" ? legacy : null;
    } catch (error) {
        void error;
        return null;
    }
}

// Current floating-scope proof from the just-accepted ready reply: the
// window's domain scope (`out/ws`) is core-untouched, its workspace row
// exists, is bound, unheld, and floating, and its output row is unheld.
// This is actual known core proof (never TS-side default inference, never
// an unknown/hold scope): an ordinary member with `floating=false` homed
// here is floating by workspace, not by intent flag. Anything else
// (opaque unknown rows, held scopes, controlled tiles) refuses.
function scopeProvesFloating(
    reply: WorldReply,
    out: string,
    ws: string,
): boolean {
    if (out === "" || ws === "") {
        return false;
    }
    const scope = reply.scopes.find((row) => row.scope === `${out}/${ws}`);
    if (scope === undefined || scope.auth !== "untouched") {
        return false;
    }
    const row = reply.workspaces.find((entry) => entry.out === out && entry.bind === ws && entry.bind !== "");
    if (row === undefined || !row.exist || row.hold !== "" || row.mode !== "floating") {
        return false;
    }
    const output = reply.outputs.find((entry) => entry.out === out);
    if (output === undefined || output.hold !== "") {
        return false;
    }
    return true;
}

// Current tiled/controlled scope proof from the just-accepted ready
// reply: the window's domain scope (`out/ws`) is controlled, its
// workspace row exists, is bound, tiled, and unheld, and its output row
// is unheld. Anything else (opaque unknown rows, held scopes, floating
// workspaces from a `defaultTiled=false` seed) refuses with no writes.
function scopeProvesTile(
    reply: WorldReply,
    out: string,
    ws: string,
): boolean {
    if (out === "" || ws === "") {
        return false;
    }
    const scope = reply.scopes.find((row) => row.scope === `${out}/${ws}`);
    if (scope === undefined || scope.auth !== "controlled") {
        return false;
    }
    const row = reply.workspaces.find((entry) => entry.out === out && entry.bind === ws && entry.bind !== "");
    if (row === undefined || !row.exist || row.hold !== "" || row.mode !== "tiled") {
        return false;
    }
    const output = reply.outputs.find((entry) => entry.out === out);
    if (output === undefined || output.hold !== "") {
        return false;
    }
    return true;
}

export class WorldBornMaximizedAdmission {
    private readonly attempts = new Set<string>();
    private readonly floatingFirst = new Set<string>();
    private readonly heldFullscreen = new Set<string>();
    private readonly settledAdmitted = new Set<string>();
    private echo: object | null = null;
    private lastMode: string | null = null;

    constructor(private readonly readModeFn?: () => unknown) {}

    get pendingEcho(): object | null {
        return this.echo;
    }

    // Prune lifetime sets on known explicit close only (the close-tracker
    // pre-ack snapshot). Minimized and merely unobserved ids are live and
    // stay put, so a restore never re-clears and id reuse starts a fresh
    // lifetime.
    pruneClosed(closedIds: ReadonlyArray<string>): void {
        for (const id of closedIds) {
            this.attempts.delete(id);
            this.floatingFirst.delete(id);
            this.heldFullscreen.delete(id);
            this.settledAdmitted.delete(id);
        }
    }

    // Settled-lifetime evidence from an accepted ready reply. The entry
    // calls this AFTER that reply's birth decision (`runAfterReady`), so
    // a first-ever birth with no prior snapshot still clears once under
    // the default. Only safe actual core controlled OR floating known
    // workspaces mark: a current reply row on a tiled/controlled scope
    // proof, or on a proven floating-workspace proof (which also covers
    // ordinary `floating=false` members). Unknown and held scopes never
    // mark, and still-held fullscreen births never mark (the held exit
    // lane owns them until its marker is consumed). Kept, fixed, and
    // floating maximizes therefore stay classified as admitted across a
    // Planner generation rebind (null previous snapshot), while genuinely
    // new ids still clear.
    noteAcceptedReply(
        fresh: WorldObservation,
        reply: WorldReply,
    ): void {
        const replyWins = new Set<string>();
        for (const row of reply.windows) {
            if (row.win !== "") {
                replyWins.add(row.win);
            }
        }
        for (const domain of fresh.domains) {
            for (const entry of domain.windows) {
                if (entry.win === "" || this.heldFullscreen.has(entry.win)) {
                    continue;
                }
                if (!replyWins.has(entry.win)) {
                    continue;
                }
                if (!scopeProvesTile(reply, entry.out, entry.ws) && !scopeProvesFloating(reply, entry.out, entry.ws)) {
                    continue;
                }
                this.settledAdmitted.add(entry.win);
            }
        }
    }

    // Post-ready admission pass over the post-accept fresh observation.
    // Returns true when at least one native clear was issued: the caller
    // then recaptures truth for apply and continues observation-only
    // catchup (never replays commands).
    runAfterReady(
        env: WorldBornMaximizedEnv,
        fresh: WorldObservation,
        reply: WorldReply,
        prevReply: WorldReply | null,
    ): boolean {
        const mode = readBornMaximizedValue(this.readModeFn);
        if (this.lastMode !== mode) {
            if (this.lastMode !== null) {
                env.log(`omnitiler:world:config-reloaded stage=born-maximized mode=${env.token(mode)}`);
            }
            this.lastMode = mode;
        }
        // Applied evidence comes ONLY from the previous accepted snapshot
        // plus the retained settled-admitted lifetime (which survives a
        // Planner generation rebind with a null previous snapshot): the
        // current new reply contains the birth row and must not
        // misclassify it as a later maximize.
        const admitted = new Set<string>();
        if (prevReply !== null) {
            for (const row of prevReply.windows) {
                admitted.add(row.win);
            }
        }
        for (const id of this.settledAdmitted) {
            admitted.add(id);
        }
        const windows: WorldObservation["domains"][number]["windows"] = [];
        for (const domain of fresh.domains) {
            for (const win of domain.windows) {
                windows.push(win);
            }
        }
        // R-MAX-03 sightings converge on every accepted reply from
        // floating observations on untiled scopes, independent of mode.
        // Ordinary `floating=false` members homed on a proven floating
        // workspace (core untouched scope plus a bound/existing/unheld
        // floating workspace row) record too: the workspace floats them,
        // not an intent flag. A scope that proves floating cancels any
        // held-fullscreen marker: the held birth ended into float, so the
        // held exit lane can never bypass this safety later.
        for (const entry of windows) {
            if (entry.win === "") {
                continue;
            }
            if ((entry.floating || scopeProvesFloating(reply, entry.out, entry.ws)) && !scopeProvesTile(reply, entry.out, entry.ws)) {
                this.floatingFirst.add(entry.win);
            }
            if (scopeProvesFloating(reply, entry.out, entry.ws)) {
                this.heldFullscreen.delete(entry.win);
            }
        }
        // Held-fullscreen birth tracking (legacy `heldInitialFullscreen`
        // parity, id-keyed): first-seen fullscreen births that are not
        // admitted wait for their exit. Later fullscreen on reserved
        // (admitted) tiles is not a birth and never enters the set, and
        // floating-first windows never enter it either (the held exit lane
        // must not bypass floating safety).
        for (const entry of windows) {
            if (entry.win !== "" && entry.fullscreen && !admitted.has(entry.win) && !this.attempts.has(entry.win) && !this.floatingFirst.has(entry.win)) {
                this.heldFullscreen.add(entry.win);
            }
        }
        // The first known non-fullscreen observation ends a held birth
        // even when the exit is not maximized: consume the marker so a
        // later user maximize is never mistaken for the held exit. The
        // maximized pass below copies and consumes its own flag, so
        // fixed-skipped and scope-skipped max exits also consume (a later
        // predicate change cannot resurrect them); unknown/held scopes
        // still never clear natively.
        for (const entry of windows) {
            if (entry.win !== "" && !entry.fullscreen && !entry.maximized) {
                this.heldFullscreen.delete(entry.win);
            }
        }
        let invalidated = false;
        let issued = false;
        for (const entry of windows) {
            if (entry.win === "" || !entry.maximized || this.attempts.has(entry.win)) {
                continue;
            }
            // Held-fullscreen exit lane: a former held birth whose
            // fullscreen first exited while still maximized clears once
            // under BOTH settings. Later maximizes of admitted tiles are
            // never held (see above) and fall through to the born lane.
            // The marker is copied and consumed for this pass, so every
            // outcome below (fixed skip, scope skip, clear) spends it. A
            // stale marker on a floating-first window is spent without
            // effect: the held lane never bypasses floating safety.
            let heldExit = !entry.fullscreen && this.heldFullscreen.has(entry.win);
            if (heldExit) {
                this.heldFullscreen.delete(entry.win);
                if (this.floatingFirst.has(entry.win)) {
                    heldExit = false;
                }
            }
            if (!heldExit) {
                if (mode !== BORN_MAXIMIZED_DEFAULT) {
                    continue;
                }
                // Still-held fullscreen waits for its exit; floating and
                // sticky identities keep their overlay.
                if (entry.fullscreen || entry.floating || entry.sticky) {
                    continue;
                }
                if (this.floatingFirst.has(entry.win)) {
                    continue;
                }
                if (admitted.has(entry.win)) {
                    continue;
                }
            }
            // CURRENT fixed predicate skips both lanes with no writes.
            if (entry.fixedAuto || entry.fixedSuppress) {
                env.log(`omnitiler:world:born-maximized-clear window=${env.token(entry.win)} outcome=skipped-fixed`);
                continue;
            }
            if (!scopeProvesTile(reply, entry.out, entry.ws)) {
                env.log(`omnitiler:world:born-maximized-clear window=${env.token(entry.win)} outcome=skipped-scope`);
                continue;
            }
            // At-most-once per safe lifetime: record BEFORE the possible
            // native call so every outcome (including a refused setter)
            // never repeats. The held exit was already consumed above.
            this.attempts.add(entry.win);
            const ref = env.liveRefById(entry.win);
            if (ref === null) {
                env.log(`omnitiler:world:born-maximized-clear window=${env.token(entry.win)} outcome=refused-observe-missing`);
                continue;
            }
            if (!invalidated) {
                invalidated = true;
                env.invalidate();
            }
            env.log(`omnitiler:world:born-maximized-clear window=${env.token(entry.win)} outcome=issued`);
            this.echo = ref;
            env.log(`omnitiler:world:born-maximized-echo-armed window=${env.token(entry.win)}`);
            let outcome = "threw";
            try {
                const method = env.readProp(ref, "setMaximize");
                if (typeof method !== "function") {
                    outcome = "refused-missing";
                } else {
                    Reflect.apply(method as (...a: ReadonlyArray<unknown>) => unknown, ref, [false, false]);
                    outcome = "invoked";
                }
            } catch (error) {
                void error;
            }
            // Same-canonical-ID readback proof: re-resolve the live ref
            // and read `maximizeMode`. Applied is claimed only on proof,
            // never on the setter outcome alone.
            let proven = false;
            try {
                const reread = env.liveRefById(entry.win);
                if (reread !== null) {
                    proven = readMaximized(env.readProp, reread) === false;
                }
            } catch (error) {
                void error;
            }
            env.log(
                `omnitiler:world:born-maximized-clear window=${env.token(entry.win)} outcome=${outcome}${proven ? "-applied" : "-unproven"}`,
            );
            if (this.echo !== null) {
                this.echo = null;
                env.log(`omnitiler:world:born-maximized-echo-cleared-no-signal window=${env.token(entry.win)}`);
            }
            issued = true;
        }
        return issued;
    }
}
